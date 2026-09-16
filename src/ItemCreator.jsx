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
    physics: tab === 'armor' ? [emptyDonor()] : '',
    replaceBones: false,
    cube: false,
    cubeSize: '0.24,0.24,0.24',
    cubeBone: 'Head',
    cubeOffset: '0,0.09,0',
    cubeWeights: 'Head:0.9,Skl_Root:0.05,Root:0.05',
    upgrades: [],
    upgradesEnabled: true,
    dyeable: false,
    skinMaterial: '',
    helperBone: '',
    armorEffects: [{ type: '', level: '' }],
});

/** Icon under public/effects for an ArmorEffectType. */
const EFFECT_ICONS = {
    AttackUp: 'AttackUp', AttackUpCold: 'AttackUpCold', AttackUpHot: 'AttackUpHot', AttackUpThunderstorm: 'AttackUpThunderstorm',
    ClimbSpeedUp: 'ClimbSpeed', ClimbSpeedUpOnlyHorizontaly: 'ClimbSpeed', ClimbWaterfall: 'SwimSpeed',
    QuietnessUp: 'Quietness', ResistBurn: 'ResistBurn', ResistCold: 'ResistCold', ResistElectric: 'ResistElectric',
    ResistFreeze: 'ResistFreeze', ResistHot: 'ResistHot', ResitLightning: 'ResistLightning',
    SandMoveUp: 'SandMove', SnowMoveUp: 'SnowMove', NotSlippy: 'SnowMove', SwimSpeedUp: 'SwimSpeed',
    MiasmaGuard: 'GloomResistance', EnableUseSwordBeam: 'ClimbSpeedAndBeamPowerUp', WakeWind: 'ResistHotAndWakeWind',
    DecreaseZonauEnergy: 'DecreaseZonauEnergy', DivingMobilityUp: 'DivingMobilityUp', RupeeGuard: 'RupeeGuard',
    LightEmission: 'Glow', NightGlow: 'Glow', Moisturizing: 'Moisturizing', MaskAll: 'MajoraMask',
    MaskBokoblin: 'MaskBokoblin', MaskHorablin: 'MaskHorablin', MaskLizalfos: 'MaskLizalfos', MaskLynel: 'MaskLynel', MaskMoriblin: 'MaskMoriblin',
    SoulPowerUpFire: 'SoulPowerUpFire', SoulPowerUpLightning: 'SoulPowerUpLightning', SoulPowerUpSpirit: 'SoulPowerUpSpirit',
    SoulPowerUpWater: 'SoulPowerUpWater', SoulPowerUpWind: 'SoulPowerUpWind', SpinAttack: 'SpinAttack', YigaDisguise: 'YigaDisguise',
};
const effectIcon = (type) => `effects/${EFFECT_ICONS[type] || 'Other'}.webp`;
/** In-game effect names (as on the armor upgrade lists); unknown types are split on capitals. */
const EFFECT_LABELS = {
    AttackUp: 'Attack Up', AttackUpCold: 'Cold Weather Attack', AttackUpHot: 'Hot Weather Attack', AttackUpThunderstorm: 'Stormy Weather Attack',
    ClimbSpeedUp: 'Climb Speed Up', ClimbSpeedUpOnlyHorizontaly: 'Climb Speed Up (sideways only)', ClimbWaterfall: 'Swim Up Waterfalls',
    QuietnessUp: 'Stealth Up', ResistBurn: 'Flame Guard', ResistCold: 'Cold Resistance', ResistElectric: 'Shock Resistance',
    ResistFreeze: 'Unfreezable', ResistHot: 'Heat Resistance', ResitLightning: 'Lightning Proof',
    SandMoveUp: 'Sand Speed Up', SnowMoveUp: 'Snow Speed Up', NotSlippy: 'Slip Resistance', SwimSpeedUp: 'Swim Speed Up',
    MiasmaGuard: 'Gloom Resistance', EnableUseSwordBeam: 'Master Sword Beam Up', WakeWind: 'Wake Wind',
    DecreaseZonauEnergy: 'Energy Up', DivingMobilityUp: 'Skydive Mobility Up', RupeeGuard: 'Rupee Padding',
    LightEmission: 'Glow', NightGlow: 'Glow (at night)', Moisturizing: 'Moisturizing', SpinAttack: 'Spin Attack',
    YigaDisguise: 'Yiga Disguise', MaskAll: 'Majora Mask', MaskBokoblin: 'Bokoblin Mask', MaskHorablin: 'Horriblin Mask',
    MaskMoriblin: 'Moblin Mask', MaskLizalfos: 'Lizalfos Mask', MaskLynel: 'Lynel Mask',
    SoulPowerUpWind: "Tulin's Sage Power Up", SoulPowerUpWater: "Sidon's Sage Power Up", SoulPowerUpFire: "Yunobo's Sage Power Up",
    SoulPowerUpLightning: "Riju's Sage Power Up", SoulPowerUpSpirit: "Mineru's Sage Power Up",
};
const effectLabel = (type) => EFFECT_LABELS[type] || type.replace(/([a-z0-9])([A-Z])/g, '$1 $2');

const MAX_UPGRADES = 4;
/**
 * How one armor physics donor is chosen: a base armor picked from the
 * catalog, an actor pack file on disk, or an actor name typed by hand. All
 * three end up as one string in the spec (`--cli create_weapon` reads a
 * vanilla actor name or a `.pack.zs` / `.pack` path).
 */
const DONOR_MODES = [
    { id: 'list', label: 'Base armor' },
    { id: 'file', label: 'Pack file' },
    { id: 'name', label: 'Actor name' },
];
const emptyDonor = (mode = 'list') => ({ mode, value: '' });
const isPackPath = (value) => /\.pack(\.zs)?$/i.test(String(value ?? '').trim());
/** Form donor entry for a spec string: files by extension, catalog armor as list picks, the rest typed. */
const donorFromSpec = (value, catalogActors = new Set()) => {
    const text = String(value ?? '').trim();
    const mode = isPackPath(text) ? 'file' : (catalogActors.has(text) ? 'list' : 'name');
    return { mode, value: text };
};
/** Physics donor strings of a form or spec (string, string list or {mode, value} list) without blanks. */
const physicsDonors = (value) => (Array.isArray(value) ? value : [value])
    .map((entry) => String((entry && typeof entry === 'object' ? entry.value : entry) ?? '').trim())
    .filter(Boolean);
const emptyUpgrade = () => ({ defense: '', rupees: '', materials: '' });
/** Splits a material list on the commas outside double quotes. */
const splitMaterials = (value) => {
    const entries = [];
    let current = '';
    let quoted = false;
    for (const char of String(value ?? '')) {
        if (char === '"') quoted = !quoted;
        if (char === ',' && !quoted) { entries.push(current); current = ''; } else current += char;
    }
    entries.push(current);
    return entries;
};
/**
 * '"Bokoblin Horn":5, Item_Ore_F:20' -> [{ actor, count }]; quoted entries are
 * pouch names resolved through the catalog, bare ones actor ids; null when
 * malformed or a name is unknown.
 */
const parseMaterials = (value, itemNames = {}) => {
    const byName = new Map(Object.entries(itemNames).map(([actor, name]) => [name.toLowerCase(), actor]));
    const materials = [];
    for (const entry of splitMaterials(value)) {
        const text = entry.trim();
        if (!text) continue;
        const match = text.match(/^(?:"([^"]*)"|([A-Za-z0-9_]+))\s*(?::\s*(\d+))?$/);
        if (!match) return null;
        const actor = match[1] !== undefined ? byName.get(match[1].trim().toLowerCase()) : match[2];
        const number = Number(match[3] ?? '1');
        if (!actor || !Number.isInteger(number) || number < 1) return null;
        materials.push({ actor, count: number });
    }
    return materials;
};
/** Materials as '"Bokoblin Horn":5, "Amber":20' (actor ids for unnamed items). */
const formatMaterials = (materials, itemNames = {}) => (materials || [])
    .map((material) => {
        const actor = Array.isArray(material) ? material[0] : (material.actor ?? material.name);
        const count = Array.isArray(material) ? material[1] : (material.count ?? material.number ?? 1);
        const name = itemNames[actor];
        return `${name ? `"${name}"` : actor}:${count}`;
    })
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
function buildSpec(tab, form, vendor, itemNames = {}) {
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
        if (form.skinMaterial) spec.skin_material = form.skinMaterial;
        if (form.helperBone) spec.helper_bone = form.helperBone;
        const effects = form.armorEffects.filter((effect) => effect.type);
        if (effects.length) {
            spec.armor_effects = effects.map((effect) => toInt(effect.level) !== undefined
                ? { type: effect.type, level: toInt(effect.level) }
                : { type: effect.type });
        }
        if (form.upgradesEnabled && form.upgrades.length) {
            spec.upgrades = form.upgrades.map((upgrade) => ({
                defense: toInt(upgrade.defense) ?? 0,
                rupees: toInt(upgrade.rupees) ?? 0,
                materials: parseMaterials(upgrade.materials, itemNames) || [],
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
function formFromSpec(spec, itemNames = {}, catalogActors = new Set()) {
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
    form.physics = isArmor
        ? (donors.length ? donors.map((donor) => donorFromSpec(donor, catalogActors)) : [emptyDonor()])
        : (donors[0] || '');
    form.replaceBones = Boolean(spec.replace_bones || spec.import_skeleton);
    if (isArmor) {
        form.kind = ARMOR_SLOTS.find((slot) => spec.actor_name.endsWith(slot.suffix))?.id || 'Head';
        form.defense = spec.defense ?? '';
        form.seriesName = spec.series_name || '';
        form.upgradesEnabled = spec.upgrades_enabled ?? spec.enable_upgrades ?? true;
        form.dyeable = Boolean(spec.dyeable || spec.make_dyeable);
        form.skinMaterial = spec.skin_material || spec.skin_material_actor || '';
        form.helperBone = spec.helper_bone || spec.helper_bones || spec.helper_bone_actor || '';
        const effects = (spec.armor_effects || spec.effects || []).map((effect) => typeof effect === 'string'
            ? { type: effect, level: '' }
            : { type: effect.type ?? effect.effect_type ?? '', level: effect.level ?? '' });
        form.armorEffects = effects.length ? effects : [{ type: '', level: '' }];
        form.upgrades = (spec.upgrades || []).map((upgrade) => ({
            defense: upgrade.defense ?? '',
            rupees: upgrade.rupees ?? upgrade.price ?? '',
            materials: formatMaterials(upgrade.materials || upgrade.items, itemNames),
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

function TemplatePicker({ templates, value, icons, onChange, onOpen, disabled, hideUpgrades, onHideUpgrades, placeholder = 'Select a template', noneLabel = null }) {
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
    const hasUpgrades = templates.some((template) => template.upgraded);
    const candidates = hideUpgrades ? templates.filter((template) => !template.upgraded) : templates;
    const visible = query
        ? candidates.filter((template) => template.actor.toLowerCase().includes(query) || template.name.toLowerCase().includes(query))
        : candidates;
    return <div className="item-creator-picker" ref={ref}>
        {hasUpgrades && onHideUpgrades && <div className="item-creator-check">
            <input id="item-creator-hide-upgrades" type="checkbox" checked={hideUpgrades} onChange={(event) => onHideUpgrades(event.target.checked)} />
            <label htmlFor="item-creator-hide-upgrades" className="item-creator-hint">Hide upgrades (Great Fairy ranks of vanilla armor)</label>
        </div>}
        <button type="button" className="item-creator-picker-button" disabled={disabled} onClick={() => { setIsOpen((open) => !open); if (!isOpen) onOpen(); }}>
            <img src={icons[value] || BLANK_ICON} alt="" onError={(event) => { event.currentTarget.onerror = null; event.currentTarget.src = BLANK_ICON; }} />
            <span className="item-creator-option-name">
                <span>{selected ? templateLabel(selected) : placeholder}</span>
                {selected && <small>{selected.actor}</small>}
            </span>
            <span className="item-creator-caret">▾</span>
        </button>
        {isOpen && <div className="item-creator-picker-list">
            <input type="text" autoFocus placeholder="Filter by name or actor…" value={filter} onChange={(event) => setFilter(event.target.value)} />
            {noneLabel && !query && <div
                className={`item-creator-option${!value ? ' selected' : ''}`}
                onClick={() => { onChange(''); setIsOpen(false); setFilter(''); }}>
                <img src={BLANK_ICON} alt="" />
                <span className="item-creator-option-name"><span>{noneLabel}</span></span>
            </div>}
            {visible.length === 0 && <div className="item-creator-empty">No templates match.</div>}
            {visible.map((template) => <div
                key={template.actor}
                className={`item-creator-option${template.actor === value ? ' selected' : ''}`}
                onClick={() => { onChange(template.actor); setIsOpen(false); setFilter(''); }}>
                <img src={icons[template.actor] || BLANK_ICON} alt="" loading="lazy" onError={(event) => { event.currentTarget.onerror = null; event.currentTarget.src = BLANK_ICON; }} />
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
    const [hideUpgrades, setHideUpgrades] = useState(true);
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
    // Helper-bone donors: every base armor of any slot whose pack carries
    // Phive/HelperBone files (a cape's bones can drive a head piece with a
    // merged skeleton, so the list is not limited to the current slot).
    const helperBoneTemplates = useMemo(() => (catalog?.templates || [])
        .filter((template) => template.helperBones && ARMOR_SLOTS.some((slot) => slot.id === template.kind)), [catalog]);
    // Physics donors picked from the list: every base armor of any slot whose
    // pack carries Phive/Cloth files (cloth physics of its own).
    const physicsTemplates = useMemo(() => (catalog?.templates || [])
        .filter((template) => template.cloth && ARMOR_SLOTS.some((slot) => slot.id === template.kind)), [catalog]);
    const physicsTemplateActors = useMemo(() => new Set(physicsTemplates.map((template) => template.actor)), [physicsTemplates]);
    // The Bargainer Statues price their goods in poes (ShopParam `MinusRupee`).
    const poeShop = (catalog?.vendors || []).find((entry) => entry.actor === vendor)?.currency === 'MinusRupee';
    const itemNames = catalog?.itemNames || {};
    const armorEffects = catalog?.armorEffects || [];
    const effectTemplates = useMemo(() => armorEffects.map((effect) => ({
        actor: effect.effectType,
        name: effectLabel(effect.effectType),
        kind: 'effect',
        upgraded: false,
        decayed: false,
    })), [armorEffects]);
    const effectIcons = useMemo(() => Object.fromEntries(armorEffects.map((effect) => [effect.effectType, effectIcon(effect.effectType)])), [armorEffects]);
    const effectLevels = (type) => armorEffects.find((effect) => effect.effectType === type)?.levels || [];
    const updateEffect = (index, patch) => update({
        armorEffects: form.armorEffects.map((effect, position) => position === index ? { ...effect, ...patch } : effect),
    });

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
    const updatePhysics = (index, patch) => update({
        physics: form.physics.map((donor, position) => position === index ? { ...donor, ...patch } : donor),
    });

    const pickFile = async (field, filters) => {
        const selected = await open({ multiple: false, directory: false, filters });
        if (typeof selected === 'string') update({ [field]: selected });
    };
    const pickPhysicsPack = async (index) => {
        const selected = await open({ multiple: false, directory: false, filters: [{ name: 'Actor pack', extensions: ['zs', 'pack'] }] });
        if (typeof selected === 'string') updatePhysics(index, { value: selected });
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
                if (parseMaterials(upgrade.materials, itemNames) === null) return `Rank ${rank}: materials must look like "Bokoblin Horn":5, "Amber":20 (pouch names in quotes, or actor ids such as Item_Enemy_77).`;
            }
        }
        return '';
    };

    const commitItem = () => {
        const problem = validate();
        setFormError(problem);
        if (problem) return;
        const spec = buildSpec(tab, form, vendor, itemNames);
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
        const loaded = formFromSpec(spec, itemNames, physicsTemplateActors);
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
            // The spec file names the mod: `C:\mods\MyCape.json` -> `MyCape`.
            const stem = path.split(/[\\/]/).pop().replace(/\.[^.]*$/, '');
            if (stem) setModName(stem);
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
                {(catalog?.vendors || []).map((entry) => <option key={entry.actor} value={entry.actor}>{entry.label}</option>)}
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
                            onChange={changeTemplate}
                            hideUpgrades={hideUpgrades}
                            onHideUpgrades={setHideUpgrades} />
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
                            <label>Skin material</label>
                            <div className="item-creator-skin-material">
                                <TemplatePicker
                                    templates={templates}
                                    value={form.skinMaterial}
                                    icons={icons}
                                    disabled={!catalog}
                                    onOpen={() => ensureIcons(form.kind, templates.map((template) => template.actor))}
                                    onChange={(actor) => update({ skinMaterial: actor })}
                                    hideUpgrades={hideUpgrades}
                                    placeholder="Keep the template's hidden skin materials"
                                    noneLabel="None (keep the template's list)" />
                                <span className="item-creator-hint">Copies the chosen actor's ArmorParam HiddenMaterialGroupList (the body-skin materials the piece hides) into the new piece.</span>
                            </div>
                            <label>Physics</label>
                            <div className="item-creator-physics">
                                {form.physics.map((donor, index) => (
                                    <div className="item-creator-physics-entry" key={index}>
                                        <select value={donor.mode} title="Where this physics donor comes from"
                                            onChange={(event) => updatePhysics(index, { mode: event.target.value, value: '' })}>
                                            {DONOR_MODES.map((mode) => <option key={mode.id} value={mode.id}>{mode.label}</option>)}
                                        </select>
                                        {donor.mode === 'list' && <TemplatePicker
                                            templates={physicsTemplates}
                                            value={donor.value}
                                            icons={icons}
                                            disabled={!catalog}
                                            onOpen={() => ensureIcons('physics', physicsTemplates.map((template) => template.actor))}
                                            onChange={(actor) => updatePhysics(index, { value: actor })}
                                            hideUpgrades={hideUpgrades}
                                            placeholder={index === 0 ? 'Base armor whose Phive / Physics files are copied (optional)' : 'Another base armor to merge physics from'}
                                            noneLabel="None" />}
                                        {donor.mode === 'file' && <div className="item-creator-path">
                                            <input type="text" value={donor.value} placeholder="Actor pack (.pack.zs / .pack) whose Phive / Physics files are copied"
                                                onChange={(event) => updatePhysics(index, { value: event.target.value })} />
                                            <button type="button" onClick={() => pickPhysicsPack(index)}>Browse…</button>
                                        </div>}
                                        {donor.mode === 'name' && <input type="text" value={donor.value} placeholder="Vanilla actor name from the RomFS, e.g. Armor_005_Head (optional)"
                                            onChange={(event) => updatePhysics(index, { value: event.target.value })} />}
                                        <button type="button" title="Remove this physics donor" disabled={form.physics.length <= 1}
                                            onClick={() => update({ physics: form.physics.filter((_, position) => position !== index) })}>−</button>
                                        <button type="button" title="Add another physics donor" hidden={index !== form.physics.length - 1}
                                            onClick={() => update({ physics: [...form.physics, emptyDonor()] })}>+</button>
                                    </div>
                                ))}
                                <span className="item-creator-hint">{physicsDonors(form.physics).length > 1
                                    ? 'Two or more donors: their cloths, skeletons and collidables are merged into one bphcl named after the actor.'
                                    : (form.physics.some((donor) => donor.mode === 'name')
                                        ? 'One donor copies its Phive / Physics files as they are. A typed name that is empty or matches no actor pack in the RomFS is skipped and the template keeps its own physics.'
                                        : 'One donor copies its Phive / Physics files as they are. Base armor lists every vanilla piece with cloth physics; a pack file can come from any mod.')}</span>
                            </div>
                            <label>Helper bones</label>
                            <div className="item-creator-skin-material">
                                <TemplatePicker
                                    templates={helperBoneTemplates}
                                    value={form.helperBone}
                                    icons={icons}
                                    disabled={!catalog}
                                    onOpen={() => ensureIcons('helper-bones', helperBoneTemplates.map((template) => template.actor))}
                                    onChange={(actor) => update({ helperBone: actor })}
                                    hideUpgrades={hideUpgrades}
                                    placeholder="Keep the helper bones the physics step leaves"
                                    noneLabel="None (keep the template's or physics donor's helper bones)" />
                                <span className="item-creator-hint">After the physics step, copies the chosen actor's Phive/HelperBone files into the piece (renamed after the actor, replacing the ones already there) and renames its ControllerSetParam, Component/Physics and ActorParam PhysicsRef to match.</span>
                            </div>
                            <label>Armor effects</label>
                            <div className="item-creator-physics">
                                {form.armorEffects.map((effect, index) => (
                                    <div className="item-creator-effect-entry" key={index}>
                                        <TemplatePicker
                                            templates={effectTemplates}
                                            value={effect.type}
                                            icons={effectIcons}
                                            disabled={!catalog}
                                            onOpen={() => {}}
                                            onChange={(type) => updateEffect(index, { type, level: effectLevels(type)[0] ?? '' })}
                                            placeholder="None"
                                            noneLabel="None" />
                                        <input type="number" min="0" value={effect.level} placeholder="Level" title="ArmorEffectLevel (vanilla uses it for DecreaseZonauEnergy)"
                                            disabled={!effect.type} onChange={(event) => updateEffect(index, { level: event.target.value })} />
                                        <button type="button" title="Remove this effect" disabled={form.armorEffects.length <= 1}
                                            onClick={() => update({ armorEffects: form.armorEffects.filter((_, position) => position !== index) })}>−</button>
                                        <button type="button" title="Add another effect" hidden={index !== form.armorEffects.length - 1}
                                            onClick={() => update({ armorEffects: [...form.armorEffects, { type: '', level: '' }] })}>+</button>
                                    </div>
                                ))}
                                <span className="item-creator-hint">{form.armorEffects.some((effect) => effect.type)
                                    ? 'ArmorParam ArmorEffect gets exactly these entries; the first one is also the PouchActorInfo ArmorEffectType shown in the inventory.'
                                    : 'None: the template\'s effects are kept. The list comes from every base armor in the RomFS.'}</span>
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
                                        <input type="text" value={upgrade.materials} placeholder='"Bokoblin Horn":5, "Amber":20' title='Materials for this step: "pouch name":count (actor ids such as Item_Enemy_77 work too)'
                                            onChange={(event) => updateUpgrade(index, { materials: event.target.value })} />
                                        <button type="button" title="Remove this rank and the ones after it" onClick={() => update({ upgrades: form.upgrades.slice(0, index) })}>×</button>
                                    </div>
                                ))}
                                <div className="item-creator-upgrade-actions">
                                    <button type="button" disabled={form.upgrades.length >= MAX_UPGRADES} onClick={() => update({ upgrades: [...form.upgrades, emptyUpgrade()] })}>Add rank</button>
                                    <button type="button" disabled={templateUpgrades.length === 0} title="Copy the template's Great Fairy chain (defense, rupees and materials per rank)"
                                        onClick={() => update({ upgrades: templateUpgrades.slice(0, MAX_UPGRADES).map((entry) => ({ defense: entry.defense ?? '', rupees: entry.rupees ?? '', materials: formatMaterials(entry.materials, itemNames) })) })}>Use template's upgrades</button>
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
                        <label>{poeShop ? 'Buy price (poes)' : 'Buy price'}</label>
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
