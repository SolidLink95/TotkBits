import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import './EsetbColorView.css';
import { useSyncExternalStore } from 'react';
import { getDocumentsSnapshot, invoke, subscribeDocuments } from './DocumentState';
import { useEditorContext } from './StateManager';

/** The backend's `file_type` of an ESETB document, as the document store records it. */
export const ESETB_FILE_TYPE = 'ESETB';

const COLOR_FIELDS = [
    { key: 'const_color0', label: 'Const color 0' },
    { key: 'const_color1', label: 'Const color 1' },
];
const ANIM_FIELDS = [
    { key: 'color_anim0', label: 'Color animation 0' },
    { key: 'color_anim1', label: 'Color animation 1' },
];
const MAX_KEYFRAMES = 8;
const APPLY_DELAY_MS = 150;

const round = (value) => Math.round((Number(value) || 0) * 10000) / 10000;
const clamp01 = (value) => Math.min(1, Math.max(0, Number(value) || 0));
/** HDR colours exceed 1.0; the swatch shows the hue at the channel maximum and keeps the intensity on pick. */
const intensityOf = (rgb) => Math.max(1, ...rgb.slice(0, 3).map((channel) => Number(channel) || 0));
const toHex = (rgb) => {
    const scale = intensityOf(rgb);
    return `#${rgb.slice(0, 3).map((channel) => Math.round(clamp01(channel / scale) * 255).toString(16).padStart(2, '0')).join('')}`;
};
const fromHex = (hex, previous) => {
    const scale = intensityOf(previous);
    return [1, 3, 5].map((index) => round(parseInt(hex.slice(index, index + 2), 16) / 255 * scale));
};
const cssColor = (rgb) => {
    const [r, g, b] = rgb.slice(0, 3).map((channel) => Math.round(clamp01(channel) * 255));
    return `rgb(${r}, ${g}, ${b})`;
};
const gradientOf = (frames) => {
    if (!frames.length) return 'transparent';
    const sorted = [...frames].sort((a, b) => a.keyframe - b.keyframe);
    const first = sorted[0].keyframe;
    const span = sorted[sorted.length - 1].keyframe - first;
    if (sorted.length === 1 || span <= 0) return cssColor(sorted[0].value);
    const stops = sorted.map((frame) => `${cssColor(frame.value)} ${((frame.keyframe - first) / span * 100).toFixed(1)}%`);
    return `linear-gradient(to right, ${stops.join(', ')})`;
};

/** A float input that only commits parseable values, so typing `0.` or `-` does not fight the caret. */
function NumberField({ value, onCommit, title, step = 0.01, className = '' }) {
    const [text, setText] = useState(String(value));
    const [focused, setFocused] = useState(false);
    useEffect(() => { if (!focused) setText(String(value)); }, [value, focused]);
    return <input
        type="number" step={step} value={text} title={title} className={`esetb-number ${className}`}
        onFocus={() => setFocused(true)}
        onBlur={() => { setFocused(false); setText(String(value)); }}
        onChange={(event) => {
            setText(event.target.value);
            const parsed = Number(event.target.value);
            if (event.target.value.trim() !== '' && Number.isFinite(parsed)) onCommit(round(parsed));
        }}
    />;
}

function RgbEditor({ rgb, onChange, alphaIndex = -1 }) {
    const channels = ['R', 'G', 'B'];
    const intensity = intensityOf(rgb);
    return <div className="esetb-rgb">
        <input
            type="color" value={toHex(rgb)} title="Pick a colour (keeps the intensity of HDR values)"
            onChange={(event) => onChange([...fromHex(event.target.value, rgb), ...rgb.slice(3)])}
        />
        {channels.map((channel, index) => <label key={channel} title={channel}>
            <span>{channel}</span>
            <NumberField value={rgb[index]} onCommit={(next) => onChange(rgb.map((current, i) => i === index ? next : current))} />
        </label>)}
        {alphaIndex >= 0 && <label title="Alpha">
            <span>A</span>
            <NumberField value={rgb[alphaIndex]} onCommit={(next) => onChange(rgb.map((current, i) => i === alphaIndex ? next : current))} />
        </label>}
        {intensity > 1 && <small title="Channel maximum above 1.0 (HDR)">×{round(intensity)}</small>}
    </div>;
}

function AnimEditor({ frames, onChange }) {
    const updateFrame = (index, patch) => onChange(frames.map((frame, i) => i === index ? { ...frame, ...patch } : frame));
    const removeFrame = (index) => onChange(frames.filter((_, i) => i !== index));
    const addFrame = () => {
        const last = frames[frames.length - 1];
        onChange([...frames, last ? { value: [...last.value], keyframe: round(last.keyframe + 1) } : { value: [1, 1, 1], keyframe: 0 }]);
    };
    return <div className="esetb-anim">
        <div className="esetb-gradient" style={{ background: gradientOf(frames) }} title="Colour over the animation" />
        {frames.map((frame, index) => <div className="esetb-keyframe" key={index}>
            <RgbEditor rgb={frame.value} onChange={(value) => updateFrame(index, { value })} />
            <label title="Keyframe time"><span>t</span>
                <NumberField value={frame.keyframe} step={1} onCommit={(keyframe) => updateFrame(index, { keyframe })} />
            </label>
            <button type="button" title="Remove keyframe" disabled={frames.length <= 1} onClick={() => removeFrame(index)}>−</button>
        </div>)}
        <button type="button" className="esetb-add" disabled={frames.length >= MAX_KEYFRAMES} title={`Add keyframe (${frames.length}/${MAX_KEYFRAMES})`} onClick={addFrame}>+ keyframe</button>
    </div>;
}

function EmitterCard({ setName, name, emitter, original, onChange }) {
    const changed = original && JSON.stringify(original) !== JSON.stringify(emitter);
    return <article className={`esetb-emitter ${changed ? 'is-changed' : ''}`}>
        <header>
            <strong>{name}</strong>
            {changed && <button type="button" title="Restore the values the YAML had when the editor opened" onClick={() => onChange(original)}>Revert</button>}
        </header>
        {COLOR_FIELDS.map(({ key, label }) => <div className="esetb-row" key={key}>
            <span className="esetb-row-label">{label}</span>
            <RgbEditor rgb={emitter[key]} alphaIndex={3} onChange={(value) => onChange({ ...emitter, [key]: value })} />
        </div>)}
        {ANIM_FIELDS.map(({ key, label }) => <div className="esetb-row esetb-row-anim" key={key}>
            <span className="esetb-row-label">{label} <small>{emitter[key].length} key{emitter[key].length === 1 ? '' : 's'}</small></span>
            <AnimEditor frames={emitter[key]} onChange={(frames) => onChange({ ...emitter, [key]: frames })} />
        </div>)}
    </article>;
}

export default function EsetbColorView() {
    const {
        activeTab, editorRef, labelTextDisplay, esetbColorsOpen, setEsetbColorsOpen, setStatusText, readOnly,
    } = useEditorContext();
    const { documents, activeDocumentId } = useSyncExternalStore(subscribeDocuments, getDocumentsSnapshot);
    const activeDocument = documents.find((document) => document.id === activeDocumentId);
    const label = labelTextDisplay.yaml || '';
    const visible = activeTab === 'YAML' && esetbColorsOpen && activeDocument?.fileType === ESETB_FILE_TYPE;
    const [document, setDocument] = useState(null);
    const [original, setOriginal] = useState(null);
    const [error, setError] = useState('');
    const [filter, setFilter] = useState('');
    const [loading, setLoading] = useState(false);
    const applyTimer = useRef(null);
    const applyQueue = useRef(Promise.resolve());

    const load = useCallback(async () => {
        const text = editorRef.current?.getValue() ?? '';
        setLoading(true);
        setError('');
        try {
            const result = await invoke('esetb_read_colors', { text });
            setDocument(result);
            setOriginal(result);
        } catch (failure) {
            setDocument(null);
            setError(String(failure));
        } finally {
            setLoading(false);
        }
    }, [editorRef]);

    useEffect(() => {
        if (!visible) return undefined;
        void load();
        return () => clearTimeout(applyTimer.current);
    }, [visible, label, load]);

    const applyToEditor = useCallback((next) => {
        clearTimeout(applyTimer.current);
        applyTimer.current = setTimeout(() => {
            applyQueue.current = applyQueue.current.then(async () => {
                const editor = editorRef.current;
                const model = editor?.getModel();
                if (!model) return;
                try {
                    const text = await invoke('esetb_apply_colors', { text: model.getValue(), document: next });
                    if (text !== model.getValue()) {
                        editor.pushUndoStop();
                        editor.executeEdits('esetb-colors', [{ range: model.getFullModelRange(), text }]);
                        editor.pushUndoStop();
                    }
                    setError('');
                    setStatusText('Colours applied to the YAML');
                } catch (failure) {
                    setError(String(failure));
                    setStatusText(`Error: ${failure}`);
                }
            });
        }, APPLY_DELAY_MS);
    }, [editorRef, setStatusText]);

    const updateEmitter = (setName, emitterName, emitter) => {
        setDocument((current) => {
            const next = { ...current, [setName]: { ...current[setName], [emitterName]: emitter } };
            applyToEditor(next);
            return next;
        });
    };

    const sets = useMemo(() => {
        if (!document) return [];
        const needle = filter.trim().toLowerCase();
        return Object.entries(document).map(([setName, emitters]) => ({
            name: setName,
            emitters: Object.entries(emitters).filter(([name]) => !needle
                || name.toLowerCase().includes(needle) || setName.toLowerCase().includes(needle)),
        })).filter((set) => set.emitters.length > 0);
    }, [document, filter]);
    const emitterCount = document ? Object.values(document).reduce((sum, emitters) => sum + Object.keys(emitters).length, 0) : 0;

    if (!visible) return null;
    return <section className="esetb-colors">
        <header>
            <div>
                <strong>{activeDocument?.title || label.split(' [')[0] || 'ESETB'}</strong>
                <small>{emitterCount} emitter{emitterCount === 1 ? '' : 's'} · colour edits are written into the YAML, save as usual</small>
            </div>
            <input type="search" placeholder="Filter emitters" value={filter} onChange={(event) => setFilter(event.target.value)} />
            <button type="button" title="Re-read the colours from the YAML text" onClick={() => void load()}>Reload</button>
            <button type="button" title="Back to the YAML editor" onClick={() => setEsetbColorsOpen(false)}>×</button>
        </header>
        {readOnly && <p className="esetb-notice">This document is read-only; the colours are shown but cannot be applied.</p>}
        {error && <p className="esetb-error">{error}</p>}
        {loading && <p className="esetb-notice">Reading emitters…</p>}
        {!loading && document && sets.length === 0 && <p className="esetb-notice">No emitter matches the filter.</p>}
        {sets.map((set) => <details className="esetb-set" key={set.name} open>
            <summary><strong>{set.name}</strong><span>{set.emitters.length} emitter{set.emitters.length === 1 ? '' : 's'}</span></summary>
            <div className="esetb-set-body">
                {set.emitters.map(([name, emitter]) => <EmitterCard
                    key={name} setName={set.name} name={name} emitter={emitter}
                    original={original?.[set.name]?.[name]}
                    onChange={readOnly ? () => {} : (next) => updateEmitter(set.name, name, next)}
                />)}
            </div>
        </details>)}
    </section>;
}
