import { invoke } from '@tauri-apps/api/core';
import { useEffect, useMemo, useState } from 'react';
import './AddElink.css';

/**
 * "Add ELink" tab of the Item creator: duplicates a vanilla ELink user (the
 * effect set an actor's ELinkParam UserName points at) under a new name, with
 * the position, rotation, scale, color, emission, bone and emitter set of
 * every asset call editable, and clones the user's Effect/<name>.esetb file
 * along with it. "Add ELink to mod" hands the request to the Item creator's
 * list; the effect is generated with the weapons and armor when the mod is
 * created, into the same `<output>/<mod>/romfs`, covered by the same RSTB
 * pass.
 */

const RAD_TO_DEG = 180 / Math.PI;
const ROTATION_KEYS = ['RotationX', 'RotationY', 'RotationZ'];
const TRANSFORM_KEYS = ['Bone', 'Scale', 'PositionX', 'PositionY', 'PositionZ', 'RotationX', 'RotationY', 'RotationZ'];
const COLOR_KEYS = ['Red', 'Green', 'Blue', 'Alpha'];
const GROUPS = [
    { title: 'Emitter set', keys: ['RuntimeAssetName', 'EffectGroup', 'EffectPauseGroup'], wide: true },
    { title: 'Transform', keys: ['Bone', 'Scale', 'PositionX', 'PositionY', 'PositionZ', 'RotationX', 'RotationY', 'RotationZ'] },
    { title: 'Color multiplier', keys: ['Red', 'Green', 'Blue', 'Alpha'] },
    { title: 'Emission and timing', keys: ['EmissionRate', 'EmissionScale', 'EmissionInterval', 'DirectionalVel', 'LifeScale', 'Delay', 'Duration'] },
];
const LABELS = {
    RuntimeAssetName: 'Emitter set (RuntimeAssetName)', EffectGroup: 'Effect group', EffectPauseGroup: 'Pause group',
    Bone: 'Bone', Scale: 'Scale', PositionX: 'Position X', PositionY: 'Position Y', PositionZ: 'Position Z',
    RotationX: 'Rotation X (°)', RotationY: 'Rotation Y (°)', RotationZ: 'Rotation Z (°)',
    Red: 'Red', Green: 'Green', Blue: 'Blue', Alpha: 'Alpha',
    EmissionRate: 'Emission rate', EmissionScale: 'Emission scale', EmissionInterval: 'Emission interval',
    DirectionalVel: 'Directional velocity', LifeScale: 'Life scale', Delay: 'Delay', Duration: 'Duration',
};

const isRotation = (key) => ROTATION_KEYS.includes(key);
/** File value -> what the input shows (rotations in degrees, 4 decimals). */
const toDisplay = (key, value) => {
    if (value == null || value === '' || value === 'CURVE') return value ?? '';
    if (!isRotation(key)) return value;
    const number = Number(value);
    return Number.isFinite(number) ? String(Math.round(number * RAD_TO_DEG * 10000) / 10000) : value;
};
/** Input text -> file value (degrees back to radians). */
const fromDisplay = (key, text) => {
    if (!isRotation(key) || text === '') return text;
    const number = Number(text);
    return Number.isFinite(number) ? String(number / RAD_TO_DEG) : text;
};

function AddElinkPanel({ active, editing, takenNames = [], onCommit, onCancelEdit }) {
    const [catalog, setCatalog] = useState(null);
    const [formError, setFormError] = useState('');
    const [catalogError, setCatalogError] = useState('');
    const [filter, setFilter] = useState('');
    const [baseUser, setBaseUser] = useState('');
    const [newName, setNewName] = useState('');
    const [nameTouched, setNameTouched] = useState(false);
    const [cloneEsetb, setCloneEsetb] = useState(true);
    const [entries, setEntries] = useState([]);
    const [entriesError, setEntriesError] = useState('');
    const [loadingEntries, setLoadingEntries] = useState(false);
    const [selectedId, setSelectedId] = useState(-1);
    /** entry id -> { param: file value } (edited values only). */
    const [edits, setEdits] = useState({});

    useEffect(() => {
        if (!active || catalog) return;
        setCatalogError('');
        invoke('elink_users')
            .then(setCatalog)
            .catch((reason) => setCatalogError(String(reason)));
    }, [active, catalog]);

    // Editing an entry of the mod list: load its request into the form.
    useEffect(() => {
        if (!editing) return;
        setBaseUser(String(editing.baseUser ?? editing.base_user ?? ''));
        setNewName(String(editing.newName ?? editing.new_name ?? ''));
        setNameTouched(true);
        setCloneEsetb(editing.cloneEsetb ?? editing.clone_esetb ?? true);
        setEdits(Object.fromEntries((editing.entries || []).map((entry) => [
            entry.id,
            Object.fromEntries(Object.entries(entry.params || {}).map(([key, value]) => [key, value == null ? '' : String(value)])),
        ])));
        setFormError('');
    }, [editing]);

    useEffect(() => {
        if (!baseUser) { setEntries([]); return undefined; }
        let cancelled = false;
        setLoadingEntries(true);
        setEntriesError('');
        invoke('elink_user_assets', { user: baseUser })
            .then((result) => { if (!cancelled) { setEntries(result); setSelectedId(result.length ? 0 : -1); } })
            .catch((reason) => { if (!cancelled) setEntriesError(String(reason)); })
            .finally(() => { if (!cancelled) setLoadingEntries(false); });
        return () => { cancelled = true; };
    }, [baseUser]);

    const specs = useMemo(() => Object.fromEntries((catalog?.params || []).map((spec) => [spec.name, spec])), [catalog]);
    const users = useMemo(() => {
        const needle = filter.trim().toLowerCase();
        const all = catalog?.users || [];
        return needle ? all.filter((user) => user.name.toLowerCase().includes(needle)) : all;
    }, [catalog, filter]);
    const baseInfo = useMemo(() => (catalog?.users || []).find((user) => user.name === baseUser), [catalog, baseUser]);
    const selected = entries.find((entry) => entry.id === selectedId) || null;
    const editedCount = Object.values(edits).filter((params) => Object.keys(params).length > 0).length;

    const chooseBase = (name) => {
        setBaseUser(name);
        setEdits({});
        if (!nameTouched) setNewName(`${name}_custom`);
    };

    /** Value an input shows: the edit, else the file value, else blank (= default). */
    const currentValue = (entry, key) => {
        const edited = edits[entry.id]?.[key];
        if (edited !== undefined) return edited;
        return entry.params[key] ?? '';
    };
    const setParam = (entryId, key, fileValue) => {
        setEdits((current) => {
            const entry = entries.find((candidate) => candidate.id === entryId);
            const original = entry?.params[key] ?? '';
            const next = { ...(current[entryId] || {}) };
            if (fileValue === original) delete next[key]; else next[key] = fileValue;
            const all = { ...current };
            if (Object.keys(next).length) all[entryId] = next; else delete all[entryId];
            return all;
        });
    };
    const resetEntry = (entryId) => setEdits((current) => { const all = { ...current }; delete all[entryId]; return all; });
    /** Copies the selected entry's current values of `keys` to every other entry. */
    const copyToAll = (keys) => {
        if (!selected) return;
        setEdits((current) => {
            const all = { ...current };
            for (const entry of entries) {
                if (entry.id === selected.id) continue;
                const next = { ...(all[entry.id] || {}) };
                for (const key of keys) {
                    const value = currentValue(selected, key);
                    if (value === 'CURVE' || entry.params[key] === 'CURVE') continue;
                    const original = entry.params[key] ?? '';
                    if (value === original) delete next[key]; else next[key] = value;
                }
                if (Object.keys(next).length) all[entry.id] = next; else delete all[entry.id];
            }
            return all;
        });
    };

    /** Validates the form and hands the request to the mod list. */
    const commit = () => {
        if (!baseUser) { setFormError('Pick a base effect.'); return; }
        const name = newName.trim();
        if (!/^[A-Za-z0-9_]+$/.test(name)) { setFormError('The new name may only contain letters, digits and underscores.'); return; }
        if (name.length >= 32) { setFormError('The new name must be shorter than 32 characters.'); return; }
        if (name === baseUser) { setFormError('The new name must differ from the base effect.'); return; }
        if (takenNames.includes(name)) { setFormError(`${name} is already in the mod.`); return; }
        setFormError('');
        onCommit({
            baseUser,
            newName: name,
            cloneEsetb,
            entries: Object.entries(edits).map(([id, params]) => ({
                id: Number(id),
                // '' clears the parameter back to the ELink default
                params: Object.fromEntries(Object.entries(params).map(([key, value]) => [key, value === '' ? null : value])),
            })),
        });
        setEdits({});
        setNameTouched(false);
        setNewName(`${baseUser}_custom`);
    };

    if (!active) return null;

    const renderField = (entry, key) => {
        const spec = specs[key];
        const raw = currentValue(entry, key);
        const edited = edits[entry.id]?.[key] !== undefined;
        const curve = raw === 'CURVE';
        const isText = spec?.kind === 'text';
        const defaultValue = spec ? (isText ? '(none)' : spec.default) : '';
        return <label className="add-elink-field" key={key}>
            <span>{LABELS[key] || key}</span>
            <input
                type={isText || curve ? 'text' : 'number'}
                step="any"
                className={`${edited ? 'edited' : ''} ${curve ? 'curve' : ''}`}
                value={curve ? 'curve-driven' : toDisplay(key, raw)}
                placeholder={defaultValue}
                disabled={curve}
                title={curve ? 'This parameter is driven by a curve in the ELink; it cannot be set to a number here.' : `Default: ${defaultValue}`}
                onChange={(event) => setParam(entry.id, key, fromDisplay(key, event.target.value))}
            />
            {spec && !isText && <span className="add-elink-default">default {spec.default}</span>}
        </label>;
    };

    return <div className="add-elink-panel">
        {catalogError && <div className="item-creator-status error">{catalogError}</div>}
        {!catalog && !catalogError && <div className="item-creator-status">Reading the ELink from the RomFS (a few seconds the first time)…</div>}
        <span className="item-creator-hint">Duplicate a vanilla effect user under a new name with moved, rescaled or recolored asset calls. The entry joins the mod list and is generated with the items when the mod is created. {catalog ? `${catalog.users.length} users in ${catalog.source}` : ''}</span>
        <div className="add-elink-body">
            <div className="add-elink-column">
                <h2>Base effect</h2>
                <input type="text" placeholder="Filter users… (e.g. Item_Weapon, Player, Enemy_)" value={filter} onChange={(event) => setFilter(event.target.value)} />
                <div className="add-elink-users">
                    {users.length === 0 && <div className="item-creator-empty">{catalog ? 'No users match.' : 'Loading…'}</div>}
                    {users.map((user) => <div
                        key={user.name}
                        className={`add-elink-user${user.name === baseUser ? ' selected' : ''}`}
                        onClick={() => chooseBase(user.name)}
                        title={user.hasEsetb ? `${user.name} has its own emitter-set file` : `${user.name} only uses shared emitter sets`}>
                        <span>{user.name}</span>
                        <small>{user.assets} call{user.assets === 1 ? '' : 's'}{user.hasEsetb ? ' · esetb' : ''}</small>
                    </div>)}
                </div>
                <div className="add-elink-options">
                    <label htmlFor="add-elink-new-name">New name</label>
                    <input id="add-elink-new-name" type="text" value={newName} placeholder="e.g. Item_Weapon_01_custom" onChange={(event) => { setNewName(event.target.value); setNameTouched(true); }} />
                    <div className="item-creator-check">
                        <input id="add-elink-clone-esetb" type="checkbox" checked={cloneEsetb} disabled={baseInfo ? !baseInfo.hasEsetb : false} onChange={(event) => setCloneEsetb(event.target.checked)} />
                        <label htmlFor="add-elink-clone-esetb" className="item-creator-hint">Clone the emitter-set file (Effect/&lt;new name&gt;.Nin_NX_NVN.esetb.byml.zs){baseInfo && !baseInfo.hasEsetb ? ' — the base has none' : ''}</label>
                    </div>
                    {formError && <div className="item-creator-error add-elink-error">{formError}</div>}
                    <div className="add-elink-create">
                        {editing && <button type="button" onClick={onCancelEdit}>Cancel edit</button>}
                        <button type="button" className="item-creator-primary" disabled={!catalog || !baseUser} onClick={commit}>
                            {editing ? 'Update ELink' : 'Add ELink to mod'}
                        </button>
                    </div>
                </div>
            </div>

            <div className="add-elink-column">
                <h2>Asset calls {baseUser ? `of ${baseUser}` : ''} {editedCount ? `· ${editedCount} edited` : ''}</h2>
                {entriesError && <div className="item-creator-status error">{entriesError}</div>}
                <div className="add-elink-entries">
                    <div className="add-elink-entry-list">
                        {!baseUser && <div className="item-creator-empty">Pick a base effect on the left.</div>}
                        {baseUser && loadingEntries && <div className="item-creator-empty">Loading…</div>}
                        {baseUser && !loadingEntries && entries.length === 0 && !entriesError && <div className="item-creator-empty">This user has no asset calls.</div>}
                        {entries.map((entry) => <div
                            key={entry.id}
                            className={`add-elink-entry${entry.id === selectedId ? ' selected' : ''}`}
                            onClick={() => setSelectedId(entry.id)}>
                            <span>{entry.path}{edits[entry.id] ? <span className="add-elink-modified"> ●</span> : null}</span>
                            <small>{currentValue(entry, 'RuntimeAssetName') || '(no emitter set)'}</small>
                        </div>)}
                    </div>
                    <div className="add-elink-params">
                        {!selected && <div className="item-creator-empty">Select an asset call to edit its parameters.</div>}
                        {selected && <>
                            <h3>{selected.assetName || selected.path}</h3>
                            <div className="add-elink-where">{selected.path}</div>
                            {GROUPS.map((group) => <div key={group.title}>
                                <div className="add-elink-group">{group.title}</div>
                                <div className={`add-elink-grid${group.wide ? ' wide' : ''}`}>
                                    {group.keys.map((key) => renderField(selected, key))}
                                </div>
                            </div>)}
                            <div className="add-elink-actions">
                                <button type="button" onClick={() => copyToAll(TRANSFORM_KEYS)} disabled={entries.length < 2}>Copy transform to all calls</button>
                                <button type="button" onClick={() => copyToAll(COLOR_KEYS)} disabled={entries.length < 2}>Copy colors to all calls</button>
                                <button type="button" onClick={() => resetEntry(selected.id)} disabled={!edits[selected.id]}>Reset this call</button>
                                <button type="button" onClick={() => setEdits({})} disabled={!editedCount}>Reset all</button>
                            </div>
                            <span className="item-creator-hint">Blank fields keep the ELink default. Position offsets are in metres relative to the bone (the actor's root when blank); <br></br>colors multiply the emitter colors, values above 1 brighten. The custom user keeps the base's triggers (ActionSlots, switches on attachment state), so it behaves like the base when an actor's ELinkParam names it.</span>
                        </>}
                    </div>
                </div>
            </div>
        </div>
    </div>;
}

export default AddElinkPanel;
