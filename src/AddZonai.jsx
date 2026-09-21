import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useMemo, useState } from 'react';
import TemplatePicker from './TemplatePicker';
import './AddZonai.css';

/**
 * "Zonai" form of the Item creator, laid out like the weapon and armor
 * forms: clones a vanilla Zonai device together with its capsule (the pouch
 * item) under new names. Every scalar of every BYML entry of the device pack
 * is editable in the Parameters section (the list comes from the pack
 * itself, `$parent`-inherited values included), the device always gets a
 * model file of its own like a weapon (the template model renamed, or a
 * custom BFRES / FBX), and any actor the device
 * references (its projectile, ...) can be cloned alongside as a "companion"
 * with edits of its own; the device is pointed at the companion. The device
 * and each companion can take over the chemistry (fire, ice, ...) of another
 * vanilla actor. The entry joins the mod list and is generated with the
 * other items.
 */

const NAME_PATTERN = /^[A-Za-z0-9_]+$/;

/** A spec `params` map without empty files. */
const compactEdits = (edits) => Object.fromEntries(Object.entries(edits || {})
    .filter(([, fields]) => fields && Object.keys(fields).length > 0));

/** `SpObj_Cannon_A_01` -> `SpObj_Cannon_A_90` (first free number from 90). */
const suggestName = (base, taken) => {
    const match = /^(.*_)(\d+)$/.exec(base);
    for (let number = 90; number < 1000; number += 1) {
        const candidate = match ? `${match[1]}${String(number).padStart(match[2].length, '0')}` : `${base}_${number}`;
        if (!taken.has(candidate)) return candidate;
    }
    return '';
};

/** The backend's default capsule name (same rule as `default_capsule_name`). */
const suggestCapsuleName = (templateActor, templateCapsule, actorName) => {
    const from = /_\d+$/.exec(templateActor)?.[0];
    const to = /_\d+$/.exec(actorName)?.[0];
    if (from && to && templateActor.slice(0, -from.length) === actorName.slice(0, -to.length) && templateCapsule.endsWith(from)) {
        return `${templateCapsule.slice(0, -from.length)}${to}`;
    }
    return `${actorName}_Capsule`;
};

/** Short label of a pack entry: `Component/ShooterParam/X.game__component__ShooterParam.bgyml` -> `ShooterParam/X`. */
const fileLabel = (path) => {
    const parts = path.split('/');
    const file = parts.pop().split('.')[0];
    const dir = parts.length ? parts[parts.length - 1] : '';
    return dir ? `${dir}/${file}` : file;
};

const sameValue = (field, value) => {
    if (field.kind === 'bool') return Boolean(value) === Boolean(field.value);
    if (field.kind === 'string') return String(value ?? '') === String(field.value ?? '');
    return Number(value) === Number(field.value);
};

const toInt = (value) => {
    const number = parseInt(String(value).trim(), 10);
    return Number.isFinite(number) ? number : undefined;
};

/**
 * One actor's parameters: the entries of its pack on the left, the scalars of
 * the selected entry on the right. `edits` is `{ [entry]: { [path]: value } }`.
 */
function ParamEditor({ inspection, edits, onChange, loading, error }) {
    const [selected, setSelected] = useState('');
    const [showShared, setShowShared] = useState(false);
    const [fileFilter, setFileFilter] = useState('');
    const [fieldFilter, setFieldFilter] = useState('');
    const files = inspection?.files || [];
    useEffect(() => {
        if (!files.length) { setSelected(''); return; }
        if (!files.some((file) => file.path === selected)) setSelected(files[0].path);
    }, [files, selected]);

    const editedFiles = new Set(Object.entries(edits || {}).filter(([, fields]) => Object.keys(fields || {}).length).map(([path]) => path));
    const fileNeedle = fileFilter.trim().toLowerCase();
    const visibleFiles = files.filter((file) => (showShared || file.own || editedFiles.has(file.path))
        && (!fileNeedle || file.path.toLowerCase().includes(fileNeedle)));
    const current = files.find((file) => file.path === selected) || null;
    const fieldNeedle = fieldFilter.trim().toLowerCase();
    const visibleFields = (current?.fields || []).filter((field) => !fieldNeedle || field.path.toLowerCase().includes(fieldNeedle));

    const setField = (field, value) => {
        onChange((all) => {
            const next = { ...(all[current.path] || {}) };
            if (value === undefined || sameValue(field, value)) delete next[field.path]; else next[field.path] = value;
            const result = { ...all };
            if (Object.keys(next).length) result[current.path] = next; else delete result[current.path];
            return result;
        });
    };
    const resetFile = () => onChange((all) => { const result = { ...all }; delete result[current.path]; return result; });
    const editedValue = (field) => edits?.[current.path]?.[field.path];

    const renderField = (field) => {
        const edited = editedValue(field);
        const isEdited = edited !== undefined;
        const value = isEdited ? edited : field.value;
        const title = `${field.path}${field.inherited ? ` (inherited from ${current.parent || 'the parent'})` : ''} · default ${String(field.value)}`;
        let input;
        if (field.kind === 'bool') {
            input = <input type="checkbox" checked={Boolean(value)} title={title} onChange={(event) => setField(field, event.target.checked)} />;
        } else if (field.kind === 'string') {
            input = <input type="text" className={isEdited ? 'edited' : ''} value={String(value ?? '')} title={title}
                onChange={(event) => setField(field, event.target.value)} />;
        } else {
            input = <input type="number" step={field.kind === 'float' ? 'any' : '1'} min={field.kind === 'uint' ? 0 : undefined}
                className={isEdited ? 'edited' : ''} value={value === '' || value == null ? '' : String(value)} title={title}
                onChange={(event) => {
                    const text = event.target.value;
                    if (text === '') { setField(field, undefined); return; }
                    const number = Number(text);
                    setField(field, Number.isFinite(number) ? number : text);
                }} />;
        }
        return <div className={`item-creator-param${isEdited ? ' edited' : ''}`} key={field.path}>
            <span className="item-creator-param-name" title={title}>{field.path}{field.inherited ? <em> inherited</em> : null}</span>
            {input}
            <button type="button" disabled={!isEdited} title={`Back to ${String(field.value)}`} onClick={() => setField(field, undefined)}>↺</button>
        </div>;
    };

    return <div className="item-creator-params">
        <div className="item-creator-param-files">
            <div className="item-creator-keys-row">
                <input type="text" placeholder="Filter files…" value={fileFilter} onChange={(event) => setFileFilter(event.target.value)} />
                <label className="item-creator-hint item-creator-param-shared" title="Entries not named after the actor are shared by other actors. Unchanged entries keep their vanilla names; an edited one is renamed after the new actor so the change stays private to it.">
                    <input type="checkbox" checked={showShared} onChange={(event) => setShowShared(event.target.checked)} /> shared
                </label>
            </div>
            <div className="item-creator-param-file-list">
                {error && <div className="item-creator-error">{error}</div>}
                {loading && <div className="item-creator-empty">Reading the pack…</div>}
                {!loading && !error && !inspection && <div className="item-creator-empty">Select a template first.</div>}
                {!loading && visibleFiles.length === 0 && inspection && <div className="item-creator-empty">No entries match.</div>}
                {visibleFiles.map((file) => <div
                    key={file.path}
                    className={`item-creator-option item-creator-param-file${file.path === selected ? ' selected' : ''}${file.own ? '' : ' shared'}`}
                    title={file.path}
                    onClick={() => setSelected(file.path)}>
                    <span className="item-creator-option-name">
                        <span>{fileLabel(file.path)}{editedFiles.has(file.path) ? <span className="item-creator-param-modified"> ●</span> : null}</span>
                        <small>{file.fields.length} value{file.fields.length === 1 ? '' : 's'}{file.own ? '' : ' · shared'}</small>
                    </span>
                </div>)}
            </div>
        </div>
        <div className="item-creator-param-values">
            {!current && <div className="item-creator-empty">Select an entry to edit its values.</div>}
            {current && <>
                <div className="item-creator-keys-row">
                    <span className="item-creator-param-where" title={current.path}>{current.path}{current.parent ? ` ← ${current.parent}` : ''}</span>
                    <input type="text" className="item-creator-param-filter" placeholder="Filter values…" value={fieldFilter} onChange={(event) => setFieldFilter(event.target.value)} />
                    <button type="button" disabled={!editedFiles.has(current.path)} onClick={resetFile}>Reset file</button>
                </div>
                <div className="item-creator-param-list">
                    {visibleFields.length === 0 && <div className="item-creator-empty">No values match.</div>}
                    {visibleFields.map(renderField)}
                </div>
            </>}
        </div>
    </div>;
}

function AddZonaiPanel({ active, catalog, vendor, poeShop, editing, takenNames = [], onCommit, onCancelEdit, onIcons }) {
    const devices = useMemo(() => catalog?.zonai || [], [catalog]);
    const templates = useMemo(() => devices.map((device) => ({
        actor: device.actor,
        name: device.name || device.actor,
        kind: 'zonai',
        upgraded: false,
        decayed: false,
    })), [devices]);
    const [icons, setIcons] = useState({});
    const [base, setBase] = useState('');
    const [actorName, setActorName] = useState('');
    const [capsuleName, setCapsuleName] = useState('');
    const [capsuleTouched, setCapsuleTouched] = useState(false);
    const [displayName, setDisplayName] = useState('');
    const [description, setDescription] = useState('');
    const [adjective, setAdjective] = useState('');
    const [modelPath, setModelPath] = useState('');
    const [fbxPath, setFbxPath] = useState('');
    const [replaceBones, setReplaceBones] = useState(false);
    const [physicsObj, setPhysicsObj] = useState('');
    const [physicsMaterial, setPhysicsMaterial] = useState('');
    const [iconPng, setIconPng] = useState('');
    const [deviceIconPng, setDeviceIconPng] = useState('');
    const [buyingPrice, setBuyingPrice] = useState('');
    const [sellingPrice, setSellingPrice] = useState('');
    const [quantity, setQuantity] = useState('1');
    const [inspection, setInspection] = useState(null);
    const [inspecting, setInspecting] = useState(false);
    const [inspectError, setInspectError] = useState('');
    const [edits, setEdits] = useState({});
    /** Vanilla actor whose Chemical entries the device takes over ('' keeps the template's). */
    const [chemical, setChemical] = useState('');
    /** Zonaite the Autobuild screen charges for the device ('' keeps the template's; vanilla devices cost 3). */
    const [autobuildCost, setAutobuildCost] = useState('3');
    /** `{ templateActor, actorName, chemical, inspection, edits, loading, error }` per companion. */
    const [companions, setCompanions] = useState([]);
    const [companionPick, setCompanionPick] = useState('');
    /** Whose values the Parameters section shows: 'device' or a companion index. */
    const [editor, setEditor] = useState('device');
    const [formError, setFormError] = useState('');

    const baseEntry = devices.find((device) => device.actor === base) || null;
    const catalogActors = useMemo(() => new Set(devices.flatMap((device) => [device.actor, device.capsule])), [devices]);
    const takenSet = useMemo(() => new Set([...takenNames, ...catalogActors]), [takenNames, catalogActors]);

    // Capsule icons (decoded from the RomFS into .cache/zonai on first use).
    useEffect(() => {
        if (!active || !devices.length || Object.keys(icons).length) return;
        const names = devices.filter((device) => device.hasIcon).map((device) => device.capsule);
        if (!names.length) return;
        invoke('item_creator_zonai_icons', { names })
            .then((result) => {
                const byDevice = Object.fromEntries(devices.map((device) => [device.actor, result[device.capsule]]).filter(([, icon]) => icon));
                setIcons(byDevice);
                if (onIcons) onIcons(byDevice);
            })
            .catch(() => {});
    }, [active, devices, icons, onIcons]);

    const inspect = useCallback((actor) => invoke('item_creator_zonai_params', { actor }), []);

    // The device's parameters follow the chosen template.
    useEffect(() => {
        if (!base) { setInspection(null); return undefined; }
        let cancelled = false;
        setInspecting(true);
        setInspectError('');
        inspect(base)
            .then((result) => { if (!cancelled) setInspection(result); })
            .catch((reason) => { if (!cancelled) setInspectError(String(reason)); })
            .finally(() => { if (!cancelled) setInspecting(false); });
        return () => { cancelled = true; };
    }, [base, inspect]);

    const loadCompanionInspection = useCallback((index, templateActor) => {
        inspect(templateActor)
            .then((result) => setCompanions((current) => current.map((companion, position) => position === index && companion.templateActor === templateActor
                ? { ...companion, inspection: result, loading: false }
                : companion)))
            .catch((reason) => setCompanions((current) => current.map((companion, position) => position === index && companion.templateActor === templateActor
                ? { ...companion, loading: false, error: String(reason) }
                : companion)));
    }, [inspect]);

    const chooseBase = (actor) => {
        const device = devices.find((entry) => entry.actor === actor);
        if (!device) return;
        setBase(device.actor);
        setEdits({});
        setChemical('');
        setCompanions([]);
        setEditor('device');
        const name = suggestName(device.actor, takenSet);
        setActorName(name);
        setCapsuleTouched(false);
        setCapsuleName(suggestCapsuleName(device.actor, device.capsule, name));
        setFormError('');
    };
    const changeActorName = (value) => {
        setActorName(value);
        if (!capsuleTouched && baseEntry) setCapsuleName(suggestCapsuleName(baseEntry.actor, baseEntry.capsule, value.trim()));
    };

    // Editing an entry of the mod list: load it into the form.
    useEffect(() => {
        if (!editing) return;
        const spec = editing;
        setBase(spec.template_actor || '');
        setActorName(spec.actor_name || '');
        setCapsuleName(spec.capsule_name || '');
        setCapsuleTouched(Boolean(spec.capsule_name));
        setDisplayName(spec.display_name || '');
        setDescription(spec.description || '');
        setAdjective(spec.adjective || '');
        setModelPath(spec.assets?.model || spec.assets?.bfres || '');
        setFbxPath(spec.assets?.fbx || '');
        setReplaceBones(Boolean(spec.assets?.replace_bones || spec.assets?.import_skeleton));
        setPhysicsObj(spec.assets?.physics_obj || spec.assets?.collision_obj || '');
        setPhysicsMaterial(spec.assets?.physics_material || spec.assets?.collision_material || '');
        setIconPng(spec.assets?.icon_png || '');
        setDeviceIconPng(spec.assets?.device_icon_png || '');
        const target = spec.vendors?.[0];
        setBuyingPrice(target?.buying_price ?? '');
        setSellingPrice(target?.selling_price ?? '');
        setQuantity(String(target?.quantity ?? 1));
        setEdits(compactEdits(spec.params));
        setChemical(spec.chemical || spec.chemical_actor || '');
        const cost = spec.autobuild_cost ?? spec.zonaite_cost;
        setAutobuildCost(cost == null ? '' : String(cost));
        const loaded = (spec.companions || []).map((companion) => ({
            templateActor: companion.template_actor || '',
            actorName: companion.actor_name || '',
            chemical: companion.chemical || companion.chemical_actor || '',
            inspection: null,
            edits: compactEdits(companion.params),
            loading: true,
            error: '',
        }));
        setCompanions(loaded);
        setEditor('device');
        setFormError('');
        loaded.forEach((companion, index) => loadCompanionInspection(index, companion.templateActor));
    }, [editing, loadCompanionInspection]);

    const addCompanion = () => {
        if (!companionPick) return;
        const taken = new Set([...takenSet, actorName.trim(), capsuleName.trim(), ...companions.map((companion) => companion.actorName)]);
        const index = companions.length;
        setCompanions((current) => [...current, {
            templateActor: companionPick,
            actorName: suggestName(companionPick, taken),
            chemical: '',
            inspection: null,
            edits: {},
            loading: true,
            error: '',
        }]);
        setEditor(index);
        setCompanionPick('');
        loadCompanionInspection(index, companionPick);
    };
    const updateCompanion = (index, patch) => setCompanions((current) => current.map((companion, position) => position === index ? { ...companion, ...patch } : companion));
    const removeCompanion = (index) => {
        setCompanions((current) => current.filter((_, position) => position !== index));
        setEditor('device');
    };

    const pickFile = async (setter, name, extensions) => {
        const selected = await open({ multiple: false, directory: false, filters: [{ name, extensions }] });
        if (typeof selected === 'string') setter(selected);
    };

    const validate = () => {
        if (!baseEntry) return 'Select a template.';
        const actor = actorName.trim();
        const capsule = capsuleName.trim();
        if (!actor) return 'Enter a device ID.';
        if (!NAME_PATTERN.test(actor)) return 'The device ID may only contain letters, digits and underscores.';
        if (!NAME_PATTERN.test(capsule)) return 'The capsule ID may only contain letters, digits and underscores.';
        if (actor === baseEntry.actor || capsule === baseEntry.capsule) return 'The new IDs must differ from the template.';
        if (actor === capsule) return 'The device and capsule IDs must differ.';
        if (actor.length > baseEntry.actor.length) return `The device ID cannot be longer than the template name (${baseEntry.actor.length} characters).`;
        if (capsule.length > baseEntry.capsule.length) return `The capsule ID cannot be longer than the template capsule (${baseEntry.capsule.length} characters).`;
        for (const name of [actor, capsule]) {
            if (catalogActors.has(name)) return `${name} already exists in the game.`;
            if (takenNames.includes(name)) return `${name} is already in the mod.`;
        }
        if (!displayName.trim()) return 'Enter a name.';
        if (!description.trim()) return 'Enter a description.';
        if (chemical.trim() && !NAME_PATTERN.test(chemical.trim())) return 'The chemical donor must be a vanilla actor ID.';
        if (autobuildCost.trim() && !/^\d+$/.test(autobuildCost.trim())) return 'The Autobuild cost must be a whole number of zonaite (or blank to keep the template\'s).';
        const names = new Set([actor, capsule]);
        for (const companion of companions) {
            const name = companion.actorName.trim();
            if (!NAME_PATTERN.test(name)) return `Companion of ${companion.templateActor}: enter a valid actor ID.`;
            if (names.has(name) || takenNames.includes(name)) return `${name} is used twice.`;
            if ((companion.chemical || '').trim() && !NAME_PATTERN.test(companion.chemical.trim())) return `${name}: the chemical donor must be a vanilla actor ID.`;
            names.add(name);
        }
        return '';
    };

    const commit = () => {
        const problem = validate();
        setFormError(problem);
        if (problem) return;
        const assets = {};
        if (modelPath.trim()) assets.model = modelPath.trim();
        if (fbxPath.trim()) assets.fbx = fbxPath.trim();
        if (fbxPath.trim() && replaceBones) assets.replace_bones = true;
        if (physicsObj.trim()) assets.physics_obj = physicsObj.trim();
        if (physicsObj.trim() && physicsMaterial.trim()) assets.physics_material = physicsMaterial.trim();
        if (iconPng.trim()) assets.icon_png = iconPng.trim();
        if (deviceIconPng.trim()) assets.device_icon_png = deviceIconPng.trim();
        const vendors = vendor ? [{
            actor_name: vendor,
            ...(toInt(buyingPrice) !== undefined ? { buying_price: toInt(buyingPrice) } : {}),
            ...(toInt(sellingPrice) !== undefined ? { selling_price: toInt(sellingPrice) } : {}),
            quantity: Math.max(1, toInt(quantity) ?? 1),
        }] : [];
        onCommit({
            kind: 'Zonai',
            actor_name: actorName.trim(),
            template_actor: baseEntry.actor,
            capsule_name: capsuleName.trim(),
            display_name: displayName.trim(),
            description: description.trim(),
            ...(adjective.trim() ? { adjective: adjective.trim() } : {}),
            params: compactEdits(edits),
            ...(chemical.trim() ? { chemical: chemical.trim() } : {}),
            ...(toInt(autobuildCost) !== undefined ? { autobuild_cost: toInt(autobuildCost) } : {}),
            companions: companions.map((companion) => ({
                template_actor: companion.templateActor,
                actor_name: companion.actorName.trim(),
                params: compactEdits(companion.edits),
                ...((companion.chemical || '').trim() ? { chemical: companion.chemical.trim() } : {}),
            })),
            assets,
            vendors,
        });
        // A fresh form for the next device of the same kind.
        setEdits({});
        setChemical('');
        setCompanions([]);
        setEditor('device');
        setDisplayName('');
        setDescription('');
        setAdjective('');
        setModelPath('');
        setFbxPath('');
        setPhysicsObj('');
        setPhysicsMaterial('');
        setIconPng('');
        setDeviceIconPng('');
        const taken = new Set([...takenSet, actorName.trim(), capsuleName.trim()]);
        const next = suggestName(baseEntry.actor, taken);
        setActorName(next);
        setCapsuleTouched(false);
        setCapsuleName(suggestCapsuleName(baseEntry.actor, baseEntry.capsule, next));
    };

    if (!active) return null;

    const editedCount = Object.keys(compactEdits(edits)).length;
    const companionCandidates = (inspection?.referencedActors || []);
    const activeCompanion = typeof editor === 'number' ? companions[editor] : null;
    const companionEdited = (companion) => Object.keys(compactEdits(companion.edits)).length;

    return <>
        <div className="item-creator-form">
            <label>Template</label>
            <TemplatePicker
                templates={templates}
                value={base}
                icons={icons}
                onChange={chooseBase}
                disabled={!templates.length}
                placeholder={templates.length ? 'Select a Zonai device' : 'No Zonai devices found in the RomFS'} />
            <label>Device ID</label>
            <input type="text" value={actorName} placeholder="e.g. SpObj_Cannon_A_90" onChange={(event) => changeActorName(event.target.value)} />
            <label>Capsule ID</label>
            <input type="text" value={capsuleName} placeholder="e.g. SpObj_Cannon_Capsule_A_90" onChange={(event) => { setCapsuleName(event.target.value); setCapsuleTouched(true); }} />
            <span className="item-creator-hint">The capsule is the pouch item (the device's icon, name and description are its); the device is what it spawns.</span>
            <label>Name</label>
            <input type="text" value={displayName} placeholder={baseEntry?.name || 'Pouch name'} onChange={(event) => setDisplayName(event.target.value)} />
            <label>Description</label>
            <textarea value={description} placeholder="Pouch description" onChange={(event) => setDescription(event.target.value)} />
            <label>Fuse adjective</label>
            <input type="text" value={adjective} placeholder="Attachment adjective (defaults to the name)" onChange={(event) => setAdjective(event.target.value)} />
            <label>Autobuild cost</label>
            <input type="number" min="0" step="1" value={autobuildCost} placeholder="Keep the template's (3 zonaite)"
                title="Zonaite charged when Autobuild rebuilds the device: an AutoBuilderReplacementParam entry (OverwriteConsumptionNum) referenced from the device's GameParameterTable, the way Obj_AirPlatform prices its sky platform at 100. Blank keeps the template's price."
                onChange={(event) => setAutobuildCost(event.target.value)} />

            <div className="item-creator-section">Parameters</div>
            <label>Values of</label>
            <div className="item-creator-keys-row item-creator-param-targets">
                <button type="button" className={editor === 'device' ? 'active' : ''} onClick={() => setEditor('device')}>
                    Device{editedCount ? ` · ${editedCount} file${editedCount === 1 ? '' : 's'} edited` : ''}
                </button>
                {companions.map((companion, index) => <button key={`${companion.templateActor}-${index}`} type="button" className={editor === index ? 'active' : ''} onClick={() => setEditor(index)}>
                    {companion.actorName || companion.templateActor}{companionEdited(companion) ? ` · ${companionEdited(companion)} edited` : ''}
                </button>)}
            </div>
            <label>Companion</label>
            <div className="item-creator-keys-row">
                <select value={companionPick} disabled={!companionCandidates.length} onChange={(event) => setCompanionPick(event.target.value)}>
                    <option value="">{companionCandidates.length ? 'Actor the device references…' : (base ? 'The device references no other actor' : 'Select a template first')}</option>
                    {companionCandidates.map((actor) => <option key={actor} value={actor}>{actor}</option>)}
                </select>
                <button type="button" disabled={!companionPick} title="Clone this actor with the device (its projectile, for example) and point the device at the clone" onClick={addCompanion}>Add companion</button>
            </div>
            {activeCompanion && <>
                <label>Companion ID</label>
                <div className="item-creator-keys-row">
                    <input type="text" value={activeCompanion.actorName} onChange={(event) => updateCompanion(editor, { actorName: event.target.value })} />
                    <span className="item-creator-hint">from {activeCompanion.templateActor}{activeCompanion.actorName.trim() === activeCompanion.templateActor ? ' (same name: the vanilla actor is edited in place, every device using it sees the change)' : ''}</span>
                    <button type="button" onClick={() => removeCompanion(editor)}>Remove</button>
                </div>
            </>}
            <label>Chemical donor</label>
            <input type="text"
                value={editor === 'device' ? chemical : (activeCompanion?.chemical || '')}
                placeholder={`Keep the chemistry of ${editor === 'device' ? (baseEntry?.actor || 'the template') : (activeCompanion?.templateActor || 'the template')}`}
                title="A vanilla actor whose Chemical entries (fire, ice, electricity...) are copied into this actor's pack and referenced by its ActorParam, e.g. Drake_Beam_Small_Fire to make a beam burn"
                onChange={(event) => (editor === 'device' ? setChemical(event.target.value) : updateCompanion(editor, { chemical: event.target.value }))} />
            <span className="item-creator-hint">Vanilla actor whose Chemical entries this {editor === 'device' ? 'device' : 'companion'} takes over (its fire, ice or electric element and how it burns), for example a fire projectile.</span>
            <div className="item-creator-param-section">
                {editor === 'device'
                    ? <ParamEditor inspection={inspection} edits={edits} onChange={setEdits} loading={inspecting} error={inspectError} />
                    : activeCompanion && <ParamEditor
                        inspection={activeCompanion.inspection}
                        edits={activeCompanion.edits}
                        onChange={(update) => setCompanions((current) => current.map((companion, position) => position === editor
                            ? { ...companion, edits: typeof update === 'function' ? update(companion.edits) : update }
                            : companion))}
                        loading={activeCompanion.loading}
                        error={activeCompanion.error} />}
            </div>
            <span className="item-creator-hint">Values keep the type they have in the file. Unchanged entries keep their vanilla names (the game's own derived actors do the same); an entry you edit is renamed after the new actor so the change stays private to it. Actor references (Work/Actor/…) can be retyped to any vanilla actor, or point at a companion by adding one.</span>

            <div className="item-creator-section">Model</div>
            <label>Model (BFRES)</label>
            <div className="item-creator-path">
                <input type="text" value={modelPath} placeholder="Clone the template model (default), or a custom .bfres / .bfres.mc" onChange={(event) => setModelPath(event.target.value)} />
                <button type="button" onClick={() => pickFile(setModelPath, 'BFRES model', ['mc', 'bfres', 'zs'])}>Browse…</button>
            </div>
            <label>Model (FBX)</label>
            <div className="item-creator-path">
                <input type="text" value={fbxPath} placeholder="Replace the geometry with an FBX (optional)" onChange={(event) => setFbxPath(event.target.value)} />
                <button type="button" onClick={() => pickFile(setFbxPath, 'FBX', ['fbx'])}>Browse…</button>
            </div>
            <label>Replace bones</label>
            <div className="item-creator-check">
                <input id="item-creator-replace-bones-zonai" type="checkbox" checked={replaceBones} disabled={!fbxPath} onChange={(event) => setReplaceBones(event.target.checked)} />
                <label htmlFor="item-creator-replace-bones-zonai" className="item-creator-hint">Replace the model's bones with the FBX skeleton</label>
            </div>
            <label>Collision (OBJ)</label>
            <div className="item-creator-path">
                <input type="text" value={physicsObj} placeholder="Keep the template collision, or an OBJ of the model's collision shape (optional)" onChange={(event) => setPhysicsObj(event.target.value)} />
                <button type="button" onClick={() => pickFile(setPhysicsObj, 'OBJ mesh', ['obj'])}>Browse…</button>
            </div>
            <label>Collision material</label>
            <input type="text" value={physicsMaterial} disabled={!physicsObj} placeholder="Material_Stone" onChange={(event) => setPhysicsMaterial(event.target.value)} />
            <span className="item-creator-hint">The OBJ (in the model's space) is split into convex pieces with CoACD (up to 40 pieces of 40 vertices; a detailed hull takes a couple of minutes) and written as the device's Polytope shape, so Ultrahand can bond to it. The rigid body's centre of mass follows; set MotionProperty to NoGravity in Parameters for a device that should float.</span>
            <label>Capsule icon (PNG)</label>
            <div className="item-creator-path">
                <input type="text" value={iconPng} placeholder="Keep the template icon" onChange={(event) => setIconPng(event.target.value)} />
                <button type="button" onClick={() => pickFile(setIconPng, 'PNG', ['png'])}>Browse…</button>
            </div>
            <label>Device icon (PNG)</label>
            <div className="item-creator-path">
                <input type="text" value={deviceIconPng} placeholder="Keep the template icon (fuse / Autobuild screens)" onChange={(event) => setDeviceIconPng(event.target.value)} />
                <button type="button" onClick={() => pickFile(setDeviceIconPng, 'PNG', ['png'])}>Browse…</button>
            </div>

            <div className="item-creator-section">Shop</div>
            <label>{poeShop ? 'Buy price (poes)' : 'Buy price'}</label>
            <input type="number" value={buyingPrice} disabled={!vendor} onChange={(event) => setBuyingPrice(event.target.value)} />
            <label>Sell price</label>
            <input type="number" value={sellingPrice} disabled={!vendor} onChange={(event) => setSellingPrice(event.target.value)} />
            <label>Stock</label>
            <input type="number" min="1" value={quantity} disabled={!vendor} onChange={(event) => setQuantity(event.target.value)} />
        </div>
        {formError && <div className="item-creator-error">{formError}</div>}
        <div className="item-creator-form-actions">
            {editing && <button type="button" onClick={onCancelEdit}>Cancel edit</button>}
            <button type="button" className="item-creator-primary" disabled={!catalog} onClick={commit}>
                {editing ? 'Update item' : 'Add Zonai device to mod'}
            </button>
        </div>
    </>;
}

export default AddZonaiPanel;
