import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import './ItemCreator.css';

const WEAPON_KINDS = [
    { id: 'SmallSword', label: 'Sword', prefix: 'Weapon_Sword_' },
    { id: 'LargeSword', label: 'Large sword', prefix: 'Weapon_Lsword_' },
    { id: 'Spear', label: 'Spear', prefix: 'Weapon_Spear_' },
    { id: 'Bow', label: 'Bow', prefix: 'Weapon_Bow_' },
    { id: 'Shield', label: 'Shield', prefix: 'Weapon_Shield_' },
];
const ARMOR_SLOTS = [
    { id: 'Head', label: 'Head', suffix: '_Head', bone: 'Head', weights: 'Head:0.9,Skl_Root:0.05,Root:0.05', offset: '0,0.09,0', size: '0.24,0.24,0.24' },
    { id: 'Upper', label: 'Upper (body)', suffix: '_Upper', bone: 'Spine_2', weights: 'Spine_2:0.9,Skl_Root:0.05,Root:0.05', offset: '0,0,0', size: '0.34,0.30,0.26' },
    { id: 'Lower', label: 'Lower (legs)', suffix: '_Lower', bone: 'Waist', weights: 'Waist:0.9,Skl_Root:0.05,Root:0.05', offset: '0,-0.06,0', size: '0.32,0.22,0.24' },
];
const BLANK_ICON = 'menu/blank.webp';

const emptyForm = (tab) => ({
    kind: tab === 'armor' ? 'Head' : 'SmallSword',
    template: '',
    actorName: '',
    displayName: '',
    description: '',
    baseName: '',
    baseAttack: '',
    maxLife: '',
    additionalDamage: '',
    shieldBashDamage: '',
    defense: '',
    seriesName: '',
    buyingPrice: '',
    sellingPrice: '',
    quantity: '1',
    fbx: '',
    iconPng: '',
    physics: tab === 'armor' ? [''] : '',
    replaceBones: false,
    cube: false,
    cubeSize: '0.24,0.24,0.24',
    cubeBone: 'Head',
    cubeOffset: '0,0.09,0',
    cubeWeights: 'Head:0.9,Skl_Root:0.05,Root:0.05',
    upgrades: [],
    upgradesEnabled: true,
    dyeable: false,
});

const MAX_UPGRADES = 4;
/** Physics donor actors of a form or spec (string or list) without blanks. */
const physicsDonors = (value) => (Array.isArray(value) ? value : [value])
    .map((entry) => String(entry ?? '').trim())
    .filter(Boolean);
const emptyUpgrade = () => ({ defense: '', rupees: '', materials: '' });
/** "Item_Enemy_77:5, Item_Ore_F:20" -> [{ actor, count }]; null when malformed. */
const parseMaterials = (value) => {
    const materials = [];
    for (const entry of String(value ?? '').split(',')) {
        const text = entry.trim();
        if (!text) continue;
        const [actor, count = '1'] = text.split(':').map((part) => part.trim());
        const number = Number(count);
        if (!/^[A-Za-z0-9_]+$/.test(actor) || !Number.isInteger(number) || number < 1) return null;
        materials.push({ actor, count: number });
    }
    return materials;
};
const formatMaterials = (materials) => (materials || [])
    .map((material) => Array.isArray(material)
        ? `${material[0]}:${material[1]}`
        : `${material.actor ?? material.name}:${material.count ?? material.number ?? 1}`)
    .join(', ');

const toInt = (value) => {
    const text = String(value ?? '').trim();
    if (text === '') return undefined;
    const number = Number(text);
    return Number.isInteger(number) ? number : undefined;
};
const toVector = (value, fallback) => {
    const parts = String(value ?? '').split(',').map((part) => Number(part.trim()));
    return parts.length === 3 && parts.every((part) => Number.isFinite(part)) ? parts : fallback;
};
const toWeights = (value) => {
    const weights = {};
    String(value ?? '').split(',').forEach((entry) => {
        const [bone, weight] = entry.split(':').map((part) => part.trim());
        if (bone && Number.isFinite(Number(weight))) weights[bone] = Number(weight);
    });
    return weights;
};

/** Spec object exactly as `--cli create_weapon` reads it. */
function buildSpec(tab, form, vendor) {
    const vendors = vendor ? [{
        actor_name: vendor,
        ...(toInt(form.buyingPrice) !== undefined ? { buying_price: toInt(form.buyingPrice) } : {}),
        ...(toInt(form.sellingPrice) !== undefined ? { selling_price: toInt(form.sellingPrice) } : {}),
        quantity: Math.max(1, toInt(form.quantity) ?? 1),
    }] : [];
    if (tab === 'armor') {
        const armorAssets = {};
        if (form.iconPng) armorAssets.icon_png = form.iconPng;
        if (form.fbx) armorAssets.fbx = form.fbx;
        const spec = {
            actor_name: form.actorName.trim(),
            template_actor: form.template,
            display_name: form.displayName.trim(),
            description: form.description.trim(),
            assets: armorAssets,
            vendors,
        };
        if (toInt(form.defense) !== undefined) spec.defense = toInt(form.defense);
        if (form.seriesName.trim()) spec.series_name = form.seriesName.trim();
        const donors = physicsDonors(form.physics);
        if (donors.length) spec.physics = donors;
        if (form.fbx && form.replaceBones) spec.replace_bones = true;
        spec.upgrades_enabled = Boolean(form.upgradesEnabled);
        if (form.dyeable) spec.dyeable = true;
        if (form.upgradesEnabled && form.upgrades.length) {
            spec.upgrades = form.upgrades.map((upgrade) => ({
                defense: toInt(upgrade.defense) ?? 0,
                rupees: toInt(upgrade.rupees) ?? 0,
                materials: parseMaterials(upgrade.materials) || [],
            }));
        }
        if (form.cube && !form.fbx) {
            spec.cube = {
                size: toVector(form.cubeSize, [0.25, 0.25, 0.25]),
                center_bone: form.cubeBone.trim() || undefined,
                offset: toVector(form.cubeOffset, [0, 0, 0]),
                weights: toWeights(form.cubeWeights),
            };
        }
        return spec;
    }
    const parameters = {};
    if (toInt(form.baseAttack) !== undefined) parameters.base_attack = toInt(form.baseAttack);
    if (toInt(form.maxLife) !== undefined) parameters.max_life = toInt(form.maxLife);
    if (toInt(form.additionalDamage) !== undefined) parameters.additional_damage = toInt(form.additionalDamage);
    if (toInt(form.shieldBashDamage) !== undefined) parameters.shield_bash_damage = toInt(form.shieldBashDamage);
    const assets = {};
    if (form.fbx) assets.fbx = form.fbx;
    if (form.iconPng) assets.icon_png = form.iconPng;
    return {
        actor_name: form.actorName.trim(),
        template_actor: form.template,
        kind: form.kind,
        display_name: form.displayName.trim(),
        description: form.description.trim(),
        ...(form.baseName.trim() ? { base_name: form.baseName.trim() } : {}),
        ...(form.physics.trim() ? { physics: form.physics.trim() } : {}),
        ...(form.fbx && form.replaceBones ? { replace_bones: true } : {}),
        weapon_parameters: parameters,
        assets,
        vendors,
    };
}

/** Inverse of buildSpec, for the Edit button. */
function formFromSpec(spec) {
    const isArmor = spec.actor_name.startsWith('Armor_');
    const tab = isArmor ? 'armor' : 'weapon';
    const form = emptyForm(tab);
    const vendor = spec.vendors?.[0];
    form.template = spec.template_actor || '';
    form.actorName = spec.actor_name || '';
    form.displayName = spec.display_name || '';
    form.description = spec.description || '';
    form.buyingPrice = vendor?.buying_price ?? '';
    form.sellingPrice = vendor?.selling_price ?? '';
    form.quantity = String(vendor?.quantity ?? 1);
    form.iconPng = spec.assets?.icon_png || '';
    form.fbx = spec.assets?.fbx || '';
    const donors = physicsDonors(spec.physics ?? spec.physics_actor);
    form.physics = isArmor ? (donors.length ? donors : ['']) : (donors[0] || '');
    form.replaceBones = Boolean(spec.replace_bones || spec.import_skeleton);
    if (isArmor) {
        form.kind = ARMOR_SLOTS.find((slot) => spec.actor_name.endsWith(slot.suffix))?.id || 'Head';
        form.defense = spec.defense ?? '';
        form.seriesName = spec.series_name || '';
        form.upgradesEnabled = spec.upgrades_enabled ?? spec.enable_upgrades ?? true;
        form.dyeable = Boolean(spec.dyeable || spec.make_dyeable);
        form.upgrades = (spec.upgrades || []).map((upgrade) => ({
            defense: upgrade.defense ?? '',
            rupees: upgrade.rupees ?? upgrade.price ?? '',
            materials: formatMaterials(upgrade.materials || upgrade.items),
        }));
        const cube = spec.cube || spec.model;
        form.cube = Boolean(cube);
        if (cube) {
            form.cubeSize = (cube.size || [0.25, 0.25, 0.25]).join(',');
            form.cubeBone = cube.center_bone || '';
            form.cubeOffset = (cube.offset || [0, 0, 0]).join(',');
            form.cubeWeights = Object.entries(cube.weights || {}).map(([bone, weight]) => `${bone}:${weight}`).join(',');
        }
    } else {
        form.kind = spec.kind || WEAPON_KINDS.find((kind) => spec.actor_name.startsWith(kind.prefix))?.id || 'SmallSword';
        form.baseName = spec.base_name || '';
        form.baseAttack = spec.weapon_parameters?.base_attack ?? '';
        form.maxLife = spec.weapon_parameters?.max_life ?? '';
        form.additionalDamage = spec.weapon_parameters?.additional_damage ?? '';
        form.shieldBashDamage = spec.weapon_parameters?.shield_bash_damage ?? '';
    }
    return { tab, form, vendor: vendor?.actor_name || '' };
}

const templateLabel = (template) => `${template.name || template.actor}${template.decayed ? ' (decayed)' : ''}`;

function TemplatePicker({ templates, value, icons, onChange, onOpen, disabled }) {
    const [isOpen, setIsOpen] = useState(false);
    const [filter, setFilter] = useState('');
    const ref = useRef(null);
    useEffect(() => {
        if (!isOpen) return undefined;
        const close = (event) => { if (ref.current && !ref.current.contains(event.target)) setIsOpen(false); };
        document.addEventListener('mousedown', close);
        return () => document.removeEventListener('mousedown', close);
    }, [isOpen]);
    const selected = templates.find((template) => template.actor === value);
    const query = filter.trim().toLowerCase();
    const visible = query
        ? templates.filter((template) => template.actor.toLowerCase().includes(query) || template.name.toLowerCase().includes(query))
        : templates;
    return <div className="item-creator-picker" ref={ref}>
        <button type="button" className="item-creator-picker-button" disabled={disabled} onClick={() => { setIsOpen((open) => !open); if (!isOpen) onOpen(); }}>
            <img src={icons[value] || BLANK_ICON} alt="" />
            <span className="item-creator-option-name">
                <span>{selected ? templateLabel(selected) : 'Select a template'}</span>
                {selected && <small>{selected.actor}</small>}
            </span>
            <span className="item-creator-caret">▾</span>
        </button>
        {isOpen && <div className="item-creator-picker-list">
            <input type="text" autoFocus placeholder="Filter by name or actor…" value={filter} onChange={(event) => setFilter(event.target.value)} />
            {visible.length === 0 && <div className="item-creator-empty">No templates match.</div>}
            {visible.map((template) => <div
                key={template.actor}
                className={`item-creator-option${template.actor === value ? ' selected' : ''}`}
                onClick={() => { onChange(template.actor); setIsOpen(false); setFilter(''); }}>
                <img src={icons[template.actor] || BLANK_ICON} alt="" loading="lazy" />
                <span className="item-creator-option-name">
                    <span>{templateLabel(template)}</span>
                    <small>{template.actor}</small>
                </span>
            </div>)}
        </div>}
    </div>;
}

function ItemCreator({ activeTab, setStatusText }) {
    const [catalog, setCatalog] = useState(null);
    const [catalogError, setCatalogError] = useState('');
    const [icons, setIcons] = useState({});
    const loadedKinds = useRef(new Set());
    const [modName, setModName] = useState('MyItems');
    const [outputDir, setOutputDir] = useState('');
    const [vendor, setVendor] = useState('');
    const [zstdLevel, setZstdLevel] = useState('');
    const [tab, setTab] = useState('weapon');
    const [form, setForm] = useState(() => emptyForm('weapon'));
    const [placeholders, setPlaceholders] = useState({ name: '', description: '' });
    const [templateUpgrades, setTemplateUpgrades] = useState([]);
    const [items, setItems] = useState([]);
    const [selectedIndex, setSelectedIndex] = useState(-1);
    const [editingIndex, setEditingIndex] = useState(-1);
    const [formError, setFormError] = useState('');
    const [generating, setGenerating] = useState(false);
    const [status, setStatus] = useState({ kind: '', text: 'Add items to the list, then create the mod.' });

    useEffect(() => {
        if (activeTab !== 'ITEM_CREATOR' || catalog) return;
        setCatalogError('');
        invoke('item_creator_catalog')
            .then((result) => {
                setCatalog(result);
                setVendor((current) => current || result.vendors.find((entry) => entry.actor === 'Npc_TripMaster_00')?.actor || result.vendors[0]?.actor || '');
            })
            .catch((reason) => setCatalogError(String(reason)));
    }, [activeTab, catalog]);

    const templates = useMemo(() => (catalog?.templates || []).filter((template) => template.kind === form.kind), [catalog, form.kind]);

    const ensureIcons = useCallback((kind, names) => {
        if (loadedKinds.current.has(kind) || names.length === 0) return;
        loadedKinds.current.add(kind);
        invoke('item_creator_icons', { names })
            .then((result) => setIcons((current) => ({ ...current, ...result })))
            .catch(() => loadedKinds.current.delete(kind));
    }, []);
    useEffect(() => {
        const names = [...new Set(items.map((item) => item.template_actor))].filter((name) => !icons[name]);
        if (names.length === 0) return;
        invoke('item_creator_icons', { names })
            .then((result) => setIcons((current) => ({ ...current, ...result })))
            .catch(() => {});
    }, [items, icons]);

    const update = (patch) => setForm((current) => ({ ...current, ...patch }));

    const suggestActorName = useCallback((kindId, currentTab, list) => {
        const taken = new Set([...(catalog?.templates || []).map((template) => template.actor), ...list.map((item) => item.actor_name)]);
        if (currentTab === 'armor') {
            const slot = ARMOR_SLOTS.find((entry) => entry.id === kindId) || ARMOR_SLOTS[0];
            const existing = list.find((item) => item.actor_name.startsWith('Armor_'));
            const project = existing ? existing.actor_name.replace(/_(Head|Upper|Lower)$/, '') : null;
            if (project && !taken.has(`${project}${slot.suffix}`)) return `${project}${slot.suffix}`;
            for (let id = 900; id < 1000; id += 1) {
                const candidate = `Armor_${id}${slot.suffix}`;
                if (!taken.has(candidate)) return candidate;
            }
            return '';
        }
        const kind = WEAPON_KINDS.find((entry) => entry.id === kindId) || WEAPON_KINDS[0];
        for (let id = 900; id < 1000; id += 1) {
            const candidate = `${kind.prefix}${id}`;
            if (!taken.has(candidate)) return candidate;
        }
        return '';
    }, [catalog]);

    const switchTab = (nextTab) => {
        setTab(nextTab);
        setEditingIndex(-1);
        setFormError('');
        setPlaceholders({ name: '', description: '' });
        const next = emptyForm(nextTab);
        next.actorName = suggestActorName(next.kind, nextTab, items);
        setForm(next);
    };

    const changeKind = (kindId) => {
        const patch = { kind: kindId, template: '', actorName: suggestActorName(kindId, tab, items) };
        if (tab === 'armor') {
            const slot = ARMOR_SLOTS.find((entry) => entry.id === kindId);
            if (slot) Object.assign(patch, { cubeBone: slot.bone, cubeWeights: slot.weights, cubeOffset: slot.offset, cubeSize: slot.size });
        }
        update(patch);
    };

    const changeTemplate = (actor) => {
        update({ template: actor });
        if (!form.actorName) update({ actorName: suggestActorName(form.kind, tab, items) });
        invoke('item_creator_template_info', { actor })
            .then((info) => {
                setPlaceholders({ name: info.name || '', description: info.description || '' });
                setTemplateUpgrades(info.upgrades || []);
                setForm((current) => current.template === actor ? {
                    ...current,
                    baseAttack: info.baseAttack ?? '',
                    maxLife: info.maxLife ?? '',
                    additionalDamage: info.additionalDamage ?? '',
                    shieldBashDamage: info.shieldBashDamage ?? '',
                    defense: current.defense || (info.defense ?? ''),
                    seriesName: info.seriesName ?? '',
                    buyingPrice: info.buyingPrice ?? '',
                    sellingPrice: info.sellingPrice ?? '',
                } : current);
            })
            .catch((reason) => setStatusText(`ERROR: ${reason}`));
    };

    const updateUpgrade = (index, patch) => update({
        upgrades: form.upgrades.map((upgrade, position) => position === index ? { ...upgrade, ...patch } : upgrade),
    });
    const updatePhysics = (index, value) => update({
        physics: form.physics.map((donor, position) => position === index ? value : donor),
    });

    const pickFile = async (field, filters) => {
        const selected = await open({ multiple: false, directory: false, filters });
        if (typeof selected === 'string') update({ [field]: selected });
    };
    const pickOutputDir = async () => {
        const selected = await open({ directory: true, multiple: false });
        if (typeof selected === 'string') setOutputDir(selected);
    };

    const validate = () => {
        const actor = form.actorName.trim();
        if (!form.template) return 'Select a template.';
        if (!actor) return 'Enter an actor ID.';
        if (!/^[A-Za-z0-9_]+$/.test(actor)) return 'The actor ID may only contain letters, digits and underscores.';
        if (tab === 'armor') {
            const slot = ARMOR_SLOTS.find((entry) => entry.id === form.kind);
            if (!actor.startsWith('Armor_') || !actor.endsWith(slot.suffix)) return `Armor IDs for this slot look like Armor_900${slot.suffix}.`;
            if (actor.replace(/_(Head|Upper|Lower)$/, '') === form.template.replace(/_(Head|Upper|Lower)$/, '')) return 'The armor ID must use a different number than its template.';
        } else {
            const kind = WEAPON_KINDS.find((entry) => entry.id === form.kind);
            if (!actor.startsWith(kind.prefix)) return `${kind.label} IDs must start with ${kind.prefix}.`;
        }
        if (actor === form.template) return 'The actor ID must differ from the template.';
        if (actor.length > form.template.length) return `The actor ID cannot be longer than the template name (${form.template.length} characters).`;
        if (catalog?.templates.some((template) => template.actor === actor)) return `${actor} already exists in the game.`;
        if (items.some((item, index) => item.actor_name === actor && index !== editingIndex)) return `${actor} is already in the mod.`;
        if (!form.displayName.trim()) return 'Enter a name.';
        if (!form.description.trim()) return 'Enter a description.';
        if (tab === 'armor' && form.cube && Object.keys(toWeights(form.cubeWeights)).length === 0) return 'Cube weights must look like Head:0.9,Root:0.1.';
        if (tab === 'armor' && form.upgradesEnabled) {
            if (form.upgrades.length > MAX_UPGRADES) return `At most ${MAX_UPGRADES} upgrade ranks are possible.`;
            for (const [index, upgrade] of form.upgrades.entries()) {
                const rank = index + 2;
                const defense = toInt(upgrade.defense);
                if (defense === undefined || defense < 0) return `Rank ${rank}: enter a defense value.`;
                const rupees = toInt(upgrade.rupees);
                if (rupees === undefined || rupees < 0) return `Rank ${rank}: enter the rupee cost (0 is allowed).`;
                if (parseMaterials(upgrade.materials) === null) return `Rank ${rank}: materials must look like Item_Enemy_77:5, Item_Ore_F:20.`;
            }
        }
        return '';
    };

    const commitItem = () => {
        const problem = validate();
        setFormError(problem);
        if (problem) return;
        const spec = buildSpec(tab, form, vendor);
        setItems((current) => {
            const next = [...current];
            if (editingIndex >= 0 && editingIndex < next.length) next[editingIndex] = spec;
            else next.push(spec);
            return next;
        });
        const nextForm = emptyForm(tab);
        nextForm.kind = form.kind;
        const nextItems = editingIndex >= 0 ? items : [...items, spec];
        nextForm.actorName = suggestActorName(form.kind, tab, nextItems);
        if (tab === 'armor') {
            const slot = ARMOR_SLOTS.find((entry) => entry.id === form.kind);
            Object.assign(nextForm, { cubeBone: slot.bone, cubeWeights: slot.weights, cubeOffset: slot.offset, cubeSize: slot.size });
        }
        setForm(nextForm);
        setPlaceholders({ name: '', description: '' });
        setEditingIndex(-1);
        setStatus({ kind: '', text: `${spec.actor_name} ${editingIndex >= 0 ? 'updated' : 'added'}. ${nextItems.length} item(s) in the mod.` });
    };

    const editItem = () => {
        const spec = items[selectedIndex];
        if (!spec) return;
        const loaded = formFromSpec(spec);
        setTab(loaded.tab);
        setForm(loaded.form);
        if (loaded.vendor) setVendor(loaded.vendor);
        setEditingIndex(selectedIndex);
        setFormError('');
        setPlaceholders({ name: '', description: '' });
    };
    const removeItem = () => {
        if (selectedIndex < 0) return;
        setItems((current) => current.filter((_, index) => index !== selectedIndex));
        if (editingIndex === selectedIndex) setEditingIndex(-1);
        setSelectedIndex(-1);
    };
    const clearItems = () => { setItems([]); setSelectedIndex(-1); setEditingIndex(-1); };

    const exportSpecs = async () => {
        const path = await save({ defaultPath: `${modName || 'items'}.json`, filters: [{ name: 'JSON', extensions: ['json'] }] });
        if (!path) return;
        try {
            await invoke('item_creator_save_specs', { path, specs: items });
            setStatus({ kind: 'ok', text: `Saved ${items.length} item(s) to ${path}` });
        } catch (reason) {
            setStatus({ kind: 'error', text: String(reason) });
        }
    };
    const importSpecs = async () => {
        const path = await open({ multiple: false, directory: false, filters: [{ name: 'JSON', extensions: ['json'] }] });
        if (typeof path !== 'string') return;
        try {
            const loaded = await invoke('item_creator_load_specs', { path });
            const list = Array.isArray(loaded) ? loaded : [loaded];
            const valid = list.filter((spec) => spec && typeof spec.actor_name === 'string' && typeof spec.template_actor === 'string');
            setItems(valid);
            setSelectedIndex(-1);
            setEditingIndex(-1);
            const vendorName = valid.find((spec) => spec.vendors?.[0]?.actor_name)?.vendors[0].actor_name;
            if (vendorName) setVendor(vendorName);
            setStatus({ kind: 'ok', text: `Loaded ${valid.length} item(s) from ${path}` });
        } catch (reason) {
            setStatus({ kind: 'error', text: String(reason) });
        }
    };

    const createMod = async () => {
        if (!outputDir) { setStatus({ kind: 'error', text: 'Choose an output folder first.' }); return; }
        if (items.length === 0) { setStatus({ kind: 'error', text: 'Add at least one item to the mod.' }); return; }
        setGenerating(true);
        setStatus({ kind: '', text: `Creating ${modName} with ${items.length} item(s)… this takes about ten seconds per item.` });
        setStatusText(`Item creator: generating ${modName}…`);
        window.dispatchEvent(new CustomEvent('totkbits:items-creator', { detail: `Creating mod ${modName} (${items.length} item(s))` }));
        try {
            const result = await invoke('item_creator_generate', {
                specs: items,
                outputDir,
                modName,
                zstdLevel: toInt(zstdLevel) ?? null,
            });
            const summary = `Mod ${modName} created in ${result.seconds.toFixed(1)} s: ${result.weapons.length} weapon(s), ${result.armors.length} armor piece(s), RSTB ${result.rstbEntries} entries.\n${result.outputRomfs}`
                + (result.warnings.length ? `\nWarnings:\n${result.warnings.join('\n')}` : '');
            setStatus({ kind: 'ok', text: summary });
            setStatusText(`Mod ${modName} created successfully`);
        } catch (reason) {
            setStatus({ kind: 'error', text: `ERROR: ${reason}` });
            setStatusText(`ERROR: ${reason}`);
        } finally {
            window.dispatchEvent(new CustomEvent('totkbits:items-creator', { detail: '' }));
            setGenerating(false);
        }
    };

    if (activeTab !== 'ITEM_CREATOR') return null;

    const kindOptions = tab === 'armor' ? ARMOR_SLOTS : WEAPON_KINDS;
    const isShield = tab === 'weapon' && form.kind === 'Shield';
    const isBow = tab === 'weapon' && form.kind === 'Bow';

    return <section className="item-creator-view" aria-labelledby="item-creator-title">
        <header className="item-creator-header">
            <div>
                <h1 id="item-creator-title">Item Creator</h1>
                <p>Clone vanilla weapons, shields, bows and armor into a standalone mod. {catalog ? `${catalog.templates.length} templates from ${catalog.romfs}` : ''}</p>
            </div>
        </header>
        {catalogError && <div className="item-creator-status error">{catalogError}</div>}
        {!catalog && !catalogError && <div className="item-creator-status">Loading templates from the RomFS…</div>}

        <div className="item-creator-toprow">
            <label htmlFor="item-creator-mod-name">Mod name</label>
            <input id="item-creator-mod-name" type="text" value={modName} onChange={(event) => setModName(event.target.value)} />
            <label htmlFor="item-creator-output">Output folder</label>
            <div className="item-creator-path">
                <input id="item-creator-output" type="text" value={outputDir} placeholder="Folder that will receive <Mod name>/romfs" onChange={(event) => setOutputDir(event.target.value)} />
                <button type="button" onClick={pickOutputDir}>Browse…</button>
            </div>
            <label htmlFor="item-creator-shop">Shop</label>
            <select id="item-creator-shop" value={vendor} onChange={(event) => setVendor(event.target.value)}>
                <option value="">Not sold</option>
                {(catalog?.vendors || []).map((entry) => <option key={entry.actor} value={entry.actor}>Beedle: {entry.location ? `${entry.location} (${entry.actor})` : entry.actor}</option>)}
                {/* {(catalog?.vendors || []).map((entry) => <option key={entry.actor} value={entry.actor}>Beedle: {entry.location ? `${entry.location} (${entry.actor})` : entry.actor}</option>)} */}
            </select>
            {/* <label htmlFor="item-creator-zstd">RSTB zstd level</label>
            <input id="item-creator-zstd" type="number" min="1" max="22" placeholder="default" value={zstdLevel} onChange={(event) => setZstdLevel(event.target.value)} /> */}
        </div>

        <div className="item-creator-body">
            <div className="item-creator-main">
                <div className="item-creator-tabs">
                    <button type="button" className={tab === 'weapon' ? 'active' : ''} onClick={() => switchTab('weapon')}>Add weapon</button>
                    <button type="button" className={tab === 'armor' ? 'active' : ''} onClick={() => switchTab('armor')}>Add armor</button>
                </div>
                <div className="item-creator-panel">
                    <div className="item-creator-form">
                        <label>{tab === 'armor' ? 'Slot' : 'Type'}</label>
                        <select value={form.kind} onChange={(event) => changeKind(event.target.value)}>
                            {kindOptions.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}
                        </select>
                        <label>Template</label>
                        <TemplatePicker
                            templates={templates}
                            value={form.template}
                            icons={icons}
                            disabled={!catalog}
                            onOpen={() => ensureIcons(form.kind, templates.map((template) => template.actor))}
                            onChange={changeTemplate} />
                        <label>Actor ID</label>
                        <input type="text" value={form.actorName} onChange={(event) => update({ actorName: event.target.value })} />
                        <label>Name</label>
                        <input type="text" value={form.displayName} placeholder={placeholders.name} onChange={(event) => update({ displayName: event.target.value })} />
                        <label>Description</label>
                        <textarea value={form.description} placeholder={placeholders.description} onChange={(event) => update({ description: event.target.value })} />
                        {tab === 'weapon' && <>
                            <label>Base name</label>
                            <input type="text" value={form.baseName} placeholder="Short noun shown by the inventory, e.g. Sword" onChange={(event) => update({ baseName: event.target.value })} />
                            <div className="item-creator-section">Stats</div>
                            <label>{isShield ? 'Guard power' : 'Attack'}</label>
                            <input type="number" value={form.baseAttack} onChange={(event) => update({ baseAttack: event.target.value })} />
                            <label>Durability</label>
                            <input type="number" value={form.maxLife} onChange={(event) => update({ maxLife: event.target.value })} />
                            {!isBow && <>
                                <label>Fuse damage</label>
                                <input type="number" value={form.additionalDamage} onChange={(event) => update({ additionalDamage: event.target.value })} />
                            </>}
                            {!isShield && !isBow && <>
                                <label>Shield bash damage</label>
                                <input type="number" value={form.shieldBashDamage} onChange={(event) => update({ shieldBashDamage: event.target.value })} />
                            </>}
                            <label>Physics</label>
                            <input type="text" value={form.physics} placeholder="Vanilla actor whose Phive / Physics files are copied (optional)" onChange={(event) => update({ physics: event.target.value })} />
                            <div className="item-creator-section">Assets</div>
                            <label>Custom fbx model</label>
                            <div className="item-creator-path">
                                <input type="text" value={form.fbx} placeholder="Keep the template mesh" onChange={(event) => update({ fbx: event.target.value })} />
                                <button type="button" onClick={() => pickFile('fbx', [{ name: 'FBX', extensions: ['fbx'] }])}>Browse…</button>
                            </div>
                            <label>Replace bones</label>
                            <div className="item-creator-check">
                                <input id="item-creator-replace-bones" type="checkbox" checked={form.replaceBones} disabled={!form.fbx} onChange={(event) => update({ replaceBones: event.target.checked })} />
                                <label htmlFor="item-creator-replace-bones" className="item-creator-hint">Replace the model's bones with the FBX skeleton</label>
                            </div>
                        </>}
                        {tab === 'armor' && <>
                            <div className="item-creator-section">Stats</div>
                            <label>Defense</label>
                            <input type="number" value={form.defense} onChange={(event) => update({ defense: event.target.value })} />
                            <label>Series</label>
                            <input type="text" value={form.seriesName} placeholder="Armor set key, e.g. Hylia" onChange={(event) => update({ seriesName: event.target.value })} />
                            <label>Physics</label>
                            <div className="item-creator-physics">
                                {form.physics.map((donor, index) => (
                                    <div className="item-creator-physics-entry" key={index}>
                                        <input type="text" value={donor} placeholder={index === 0 ? 'Vanilla actor whose Phive / Physics files are copied (optional)' : 'Another vanilla actor to merge physics from'}
                                            onChange={(event) => updatePhysics(index, event.target.value)} />
                                        <button type="button" title="Remove this physics donor" disabled={form.physics.length <= 1}
                                            onClick={() => update({ physics: form.physics.filter((_, position) => position !== index) })}>−</button>
                                        <button type="button" title="Add another physics donor" hidden={index !== form.physics.length - 1}
                                            onClick={() => update({ physics: [...form.physics, ''] })}>+</button>
                                    </div>
                                ))}
                                <span className="item-creator-hint">{form.physics.filter((entry) => entry.trim()).length > 1
                                    ? 'Two or more donors: their cloths, skeletons and collidables are merged into one bphcl named after the actor.'
                                    : 'One donor copies its Phive / Physics files as they are.'}</span>
                            </div>
                            <label>Dye</label>
                            <div className="item-creator-check">
                                <input id="item-creator-dyeable" type="checkbox" checked={form.dyeable} onChange={(event) => update({ dyeable: event.target.checked })} />
                                <label htmlFor="item-creator-dyeable" className="item-creator-hint">Make dyeable (adds the ColorVariation component, sixteen albedo slices, the dye animation and the fifteen icon variants when the template is not dyeable already)</label>
                            </div>
                            <div className="item-creator-section">Great Fairy upgrades</div>
                            <label>Upgrades</label>
                            <div className="item-creator-check">
                                <input id="item-creator-upgrades-enabled" type="checkbox" checked={form.upgradesEnabled} onChange={(event) => update({ upgradesEnabled: event.target.checked })} />
                                <label htmlFor="item-creator-upgrades-enabled" className="item-creator-hint">Enable upgrades: rank actors {form.actorName.trim() || 'Armor_900_Head'}_1 … _4; an empty list inherits the template's chain (Hylian Hood values when it has none)</label>
                            </div>
                            {form.upgradesEnabled && <>
                            <label>Ranks</label>
                            <div className="item-creator-upgrades">
                                {form.upgrades.map((upgrade, index) => (
                                    <div className="item-creator-upgrade" key={index}>
                                        <span className="item-creator-upgrade-rank">{'★'.repeat(index + 1)}</span>
                                        <input type="number" value={upgrade.defense} placeholder="Defense" title="Defense at this rank"
                                            onChange={(event) => updateUpgrade(index, { defense: event.target.value })} />
                                        <input type="number" value={upgrade.rupees} placeholder="Rupees" title="Rupees the Great Fairy charges for this step"
                                            onChange={(event) => updateUpgrade(index, { rupees: event.target.value })} />
                                        <input type="text" value={upgrade.materials} placeholder="Item_Enemy_77:5, Item_Ore_F:20" title="Materials for this step, actor:count"
                                            onChange={(event) => updateUpgrade(index, { materials: event.target.value })} />
                                        <button type="button" title="Remove this rank and the ones after it" onClick={() => update({ upgrades: form.upgrades.slice(0, index) })}>×</button>
                                    </div>
                                ))}
                                <div className="item-creator-upgrade-actions">
                                    <button type="button" disabled={form.upgrades.length >= MAX_UPGRADES} onClick={() => update({ upgrades: [...form.upgrades, emptyUpgrade()] })}>Add rank</button>
                                    <button type="button" disabled={templateUpgrades.length === 0} title="Copy the template's Great Fairy chain (defense, rupees and materials per rank)"
                                        onClick={() => update({ upgrades: templateUpgrades.slice(0, MAX_UPGRADES).map((entry) => ({ defense: entry.defense ?? '', rupees: entry.rupees ?? '', materials: formatMaterials(entry.materials) })) })}>Use template's upgrades</button>
                                    <span className="item-creator-hint">{form.upgrades.length ? `${form.upgrades.length} custom rank(s) (the rest of the chain is dropped)` : 'Empty: the four ranks are derived from the template.'}</span>
                                </div>
                            </div>
                            </>}
                            {/* <div className="item-creator-section">Model</div> */}
                            {/* <label>Placeholder cube</label>
                            <div className="item-creator-check">
                                <input id="item-creator-cube" type="checkbox" checked={form.cube} onChange={(event) => update({ cube: event.target.checked })} />
                                <label htmlFor="item-creator-cube" className="item-creator-hint">Replace the template mesh with a skinned cube</label>
                            </div>
                            {form.cube && <>
                                <label>Cube size</label>
                                <input type="text" value={form.cubeSize} placeholder="x,y,z in metres" onChange={(event) => update({ cubeSize: event.target.value })} />
                                <label>Center bone</label>
                                <input type="text" value={form.cubeBone} onChange={(event) => update({ cubeBone: event.target.value })} />
                                <label>Offset</label>
                                <input type="text" value={form.cubeOffset} placeholder="x,y,z" onChange={(event) => update({ cubeOffset: event.target.value })} />
                                <label>Weights</label>
                                <input type="text" value={form.cubeWeights} placeholder="Bone:weight,Bone:weight" onChange={(event) => update({ cubeWeights: event.target.value })} />
                            </>} */}
                            <div className="item-creator-section">Assets</div>
                            <label>Custom fbx model</label>
                            <div className="item-creator-path">
                                <input type="text" value={form.fbx} placeholder="Keep the template mesh" onChange={(event) => update({ fbx: event.target.value })} />
                                <button type="button" onClick={() => pickFile('fbx', [{ name: 'FBX', extensions: ['fbx'] }])}>Browse…</button>
                            </div>
                            <label>Replace bones</label>
                            <div className="item-creator-check">
                                <input id="item-creator-replace-bones-armor" type="checkbox" checked={form.replaceBones} disabled={!form.fbx} onChange={(event) => update({ replaceBones: event.target.checked })} />
                                <label htmlFor="item-creator-replace-bones-armor" className="item-creator-hint">Replace the model's bones with the FBX skeleton</label>
                            </div>
                        </>}
                        <label>Icon (PNG)</label>
                        <div className="item-creator-path">
                            <input type="text" value={form.iconPng} placeholder="Keep the template icon" onChange={(event) => update({ iconPng: event.target.value })} />
                            <button type="button" onClick={() => pickFile('iconPng', [{ name: 'PNG', extensions: ['png'] }])}>Browse…</button>
                        </div>
                        <div className="item-creator-section">Shop</div>
                        <label>Buy price</label>
                        <input type="number" value={form.buyingPrice} disabled={!vendor} onChange={(event) => update({ buyingPrice: event.target.value })} />
                        <label>Sell price</label>
                        <input type="number" value={form.sellingPrice} disabled={!vendor} onChange={(event) => update({ sellingPrice: event.target.value })} />
                        <label>Stock</label>
                        <input type="number" min="1" value={form.quantity} disabled={!vendor} onChange={(event) => update({ quantity: event.target.value })} />
                    </div>
                    {formError && <div className="item-creator-error">{formError}</div>}
                    <div className="item-creator-form-actions">
                        {editingIndex >= 0 && <button type="button" onClick={() => { setEditingIndex(-1); setForm(emptyForm(tab)); setFormError(''); }}>Cancel edit</button>}
                        <button type="button" className="item-creator-primary" disabled={!catalog} onClick={commitItem}>
                            {editingIndex >= 0 ? 'Update item' : (tab === 'armor' ? 'Add armor to mod' : 'Add weapon to mod')}
                        </button>
                    </div>
                </div>
            </div>

            <div className="item-creator-content">
                <h2>Mod content</h2>
                <div className="item-creator-list">
                    {items.length === 0 && <div className="item-creator-empty">No items yet.</div>}
                    {items.map((item, index) => <div
                        key={`${item.actor_name}-${index}`}
                        className={`item-creator-item${index === selectedIndex ? ' selected' : ''}`}
                        onClick={() => setSelectedIndex(index)}
                        onDoubleClick={() => { setSelectedIndex(index); setTimeout(editItem, 0); }}>
                        <img src={icons[item.template_actor] || BLANK_ICON} alt="" />
                        <span className="item-creator-option-name">
                            <span>{item.actor_name}</span>
                            <small>{item.display_name}</small>
                        </span>
                    </div>)}
                </div>
                <div className="item-creator-content-actions">
                    <button type="button" disabled={selectedIndex < 0} onClick={editItem}>Edit</button>
                    <button type="button" disabled={selectedIndex < 0} onClick={removeItem}>Remove from mod</button>
                    <button type="button" disabled={items.length === 0} onClick={clearItems}>Clear list</button>
                    <button type="button" disabled={items.length === 0} onClick={exportSpecs}>Export JSON…</button>
                    <button type="button" onClick={importSpecs}>Import JSON…</button>
                </div>
            </div>
        </div>

        <footer className="item-creator-footer">
            <button type="button" className="item-creator-primary" disabled={generating || items.length === 0 || !catalog} onClick={createMod}>
                {generating ? 'Creating mod…' : 'Create mod'}
            </button>
            <div className={`item-creator-status ${status.kind}`}>{status.text}</div>
        </footer>
    </section>;
}

export default ItemCreator;
