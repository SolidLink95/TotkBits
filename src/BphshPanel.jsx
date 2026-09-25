import { open, save } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { invoke } from './DocumentState';

// Material table of an opened BPHSH mesh shape (3D tab): one row per
// material group of the shape, editable in place, plus OBJ export and
// geometry replacement. Rows go back to Rust as they are shown; the backend
// validates names and rebuilds the preview, and Save / Save As writes the
// file.

const splitList = (text) => text.split(',').map((value) => value.trim()).filter(Boolean);
const listText = (values) => (values || []).join(', ');
const toHex = (color) => {
    if (!color) return '#8d939c';
    const channel = (value) => Math.round(Math.max(0, Math.min(1, value)) * 255).toString(16).padStart(2, '0');
    return `#${channel(color[0])}${channel(color[1])}${channel(color[2])}`;
};

function ListField({ value, names, placeholder, onChange, disabled }) {
    const [text, setText] = useState(listText(value));
    useEffect(() => { setText(listText(value)); }, [value]);
    const commit = () => {
        const next = splitList(text);
        if (listText(next) !== listText(value)) onChange(next);
    };
    return <div className="bphsh-field">
        <input
            type="text"
            value={text}
            placeholder={placeholder}
            disabled={disabled}
            onChange={(event) => setText(event.target.value)}
            onBlur={commit}
            onKeyDown={(event) => { if (event.key === 'Enter') { event.currentTarget.blur(); } }}
            spellCheck={false}
        />
        <select value="" disabled={disabled} onChange={(event) => {
            const name = event.target.value;
            if (!name) return;
            const current = splitList(text);
            if (!current.includes(name)) onChange([...current, name]);
        }} aria-label="Add entry">
            <option value="">+ add…</option>
            {names.map((name) => <option key={name} value={name}>{name}</option>)}
        </select>
    </div>;
}

export default function BphshPanel({ documentId, documentTitle, revision, colors, setStatusText, onShapeChanged }) {
    const [info, setInfo] = useState(null);
    const [rows, setRows] = useState([]);
    const [dirty, setDirty] = useState(false);
    const [busy, setBusy] = useState('');
    const [error, setError] = useState('');

    useEffect(() => {
        if (!documentId) return undefined;
        let cancelled = false;
        invoke('bphsh_info', { documentId }).then((value) => {
            if (cancelled) return;
            setInfo(value);
            setRows(value.materials);
            setDirty(false);
            setError('');
        }).catch((reason) => {
            if (!cancelled) setError(String(reason));
        });
        return () => { cancelled = true; };
    }, [documentId, revision]);

    const updateRow = (index, patch) => {
        setRows((current) => current.map((row, i) => (i === index ? { ...row, ...patch } : row)));
        setDirty(true);
    };

    const applyMaterials = async () => {
        if (!documentId || busy) return;
        setBusy('apply');
        setError('');
        try {
            const materials = rows.map((row) => ({
                key: row.key,
                mat_name: row.mat_name,
                mat_flags: row.mat_flags,
                col_disable_flags: row.col_disable_flags,
            }));
            const value = await invoke('bphsh_apply_materials', { documentId, materials });
            setInfo(value);
            setRows(value.materials);
            setDirty(false);
            setStatusText('Materials applied. Use Save or Save As to write the BPHSH.');
            onShapeChanged?.();
        } catch (reason) {
            setError(String(reason));
            setStatusText(`Material update failed: ${reason}`);
        } finally {
            setBusy('');
        }
    };

    const exportObj = async () => {
        if (!documentId || busy) return;
        const stem = (documentTitle || 'shape').replace(/\.zs$/i, '').replace(/\.bphsh$/i, '').replace(/\.Nin_NX_NVN$/i, '').replace(/[^\w.-]+/g, '_');
        const output = await save({
            defaultPath: `${stem}.obj`,
            filters: [{ name: 'Wavefront OBJ', extensions: ['obj'] }],
        });
        if (!output) return;
        setBusy('export');
        setError('');
        try {
            const written = await invoke('bphsh_export_obj', { documentId, output });
            setStatusText(`Exported OBJ ${written}`);
        } catch (reason) {
            setError(String(reason));
            setStatusText(`OBJ export failed: ${reason}`);
        } finally {
            setBusy('');
        }
    };

    const replaceObj = async () => {
        if (!documentId || busy) return;
        const obj = await open({ multiple: false, filters: [{ name: 'Wavefront OBJ', extensions: ['obj'] }] });
        if (!obj) return;
        const operationId = `bphsh-replace:${documentId}:${crypto.randomUUID()}`;
        window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
            detail: { id: operationId, label: `Rebuilding ${documentTitle || 'mesh shape'} from OBJ…` },
        }));
        setBusy('replace');
        setError('');
        try {
            await new Promise((resolve) => requestAnimationFrame(resolve));
            const value = await invoke('bphsh_replace_obj', { documentId, obj });
            setInfo(value);
            setRows(value.materials);
            setDirty(false);
            setStatusText(`Geometry replaced: ${value.triangles.toLocaleString()} triangles, ${value.materials.length} materials. Use Save or Save As to write the BPHSH.`);
            onShapeChanged?.();
        } catch (reason) {
            setError(String(reason));
            setStatusText(`OBJ replacement failed: ${reason}`);
        } finally {
            setBusy('');
            window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                detail: { id: operationId, done: true },
            }));
        }
    };

    return <>
        <section className="bfres-export-panel bphsh-panel">
            <header><strong>Collision</strong>{info && <small>{info.triangles.toLocaleString()} triangles · {info.sections} sections · {info.materials.length} materials</small>}</header>
            <button type="button" onClick={applyMaterials} disabled={!info || !dirty || Boolean(busy)} className={dirty ? 'active' : ''}>
                {busy === 'apply' ? 'Applying…' : 'Apply materials'}
            </button>
            <button type="button" onClick={exportObj} disabled={!info || Boolean(busy)}>
                {busy === 'export' ? 'Exporting…' : 'Export OBJ'}
            </button>
            <button type="button" onClick={replaceObj} disabled={!info || Boolean(busy)}>
                {busy === 'replace' ? 'Replacing…' : 'Replace (OBJ)'}
            </button>
        </section>
        {error && <p className="bphsh-error" role="alert">{error}</p>}
        {info && <div className="bphsh-table">
            <table>
                <thead>
                    <tr><th>Group</th><th>Material</th><th>Shape tags</th><th>No collision with</th></tr>
                </thead>
                <tbody>
                    {rows.map((row, index) => <tr key={row.key}>
                        <td>
                            <span className="bphsh-swatch" style={{ background: toHex(colors?.[row.key]) }} />
                            <strong>{row.key}</strong>
                            <small>{row.triangles.toLocaleString()} tris</small>
                        </td>
                        <td>
                            <select value={info.material_names.includes(row.mat_name) ? row.mat_name : '__custom'} disabled={Boolean(busy)} onChange={(event) => {
                                if (event.target.value !== '__custom') updateRow(index, { mat_name: event.target.value });
                            }}>
                                {!info.material_names.includes(row.mat_name) && <option value="__custom">{row.mat_name || '(id)'}</option>}
                                {info.material_names.map((name) => <option key={name} value={name}>{name}</option>)}
                            </select>
                        </td>
                        <td>
                            <ListField value={row.mat_flags} names={info.flag_names} placeholder="none" disabled={Boolean(busy)} onChange={(mat_flags) => updateRow(index, { mat_flags })} />
                        </td>
                        <td>
                            <ListField value={row.col_disable_flags} names={info.layer_names} placeholder="collides with all" disabled={Boolean(busy)} onChange={(col_disable_flags) => updateRow(index, { col_disable_flags })} />
                        </td>
                    </tr>)}
                </tbody>
            </table>
            <p className="bphsh-note">Names come from PhiveConfig; hex literals (0x40) stand for unnamed bits. Save writes the edited shape; a plain .bphsh name saves it uncompressed.</p>
        </div>}
    </>;
}
