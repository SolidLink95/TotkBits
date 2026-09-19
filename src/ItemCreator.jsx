import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import AddElinkPanel from './AddElink';
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
/** List icon of a custom ELink effect entry. */
const ELINK_ICON = 'effects/Glow.webp';
/** A mod list entry made on the Add ELink tab (`{baseUser, newName, cloneEsetb, entries}`). */
const isElinkSpec = (spec) => Boolean(spec) && typeof (spec.baseUser ?? spec.base_user) === 'string';
const elinkName = (spec) => String(spec.newName ?? spec.new_name ?? '');
const elinkBase = (spec) => String(spec.baseUser ?? spec.base_user ?? '');

const emptyForm = (tab) => ({
    kind: tab === 'armor' ? 'Head' : 'SmallSword',
    template: '',
    actorName: '',
    displayName: '',
    description: '',
    baseName: '',
    baseAttack: '',
    maxLife: '',
    indestructible: false,
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
    audio: '',
    effect: emptyLink(),
    effectAi: false,
    effectKeys: '',
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
/**
 * How the Effect (ELink) of an item is chosen: a vanilla actor whose ELink
 * file is copied, an ELinkParam .bgyml / actor pack file used as it is, or
 * a bare ELink user name (e.g. one made with the Add ELink tab).
 */
const LINK_MODES = [
    { id: 'name', label: 'Actor name' },
    { id: 'file', label: 'File' },
    { id: 'user', label: 'User name' },
];
const emptyLink = (mode = 'name') => ({ mode, value: '' });
const isLinkFilePath = (value) => /\.(bgyml|byml|pack|zs)$/i.test(String(value ?? '').trim()) || /[\\/]/.test(String(value ?? ''));
/** Spec `effect` value of a form link entry: an actor name string, `{source: "file", path}` or `{source: "user_name", user_name}`; undefined when blank. */
const linkToSpec = (link) => {
    const value = String(link?.value ?? '').trim();
    if (!value) return undefined;
    if (link.mode === 'file') return { source: 'file', path: value };
    if (link.mode === 'user') return { source: 'user_name', user_name: value };
    return value;
};
/** Form link entry of a spec `effect` value (plain string or tagged object). */
const linkFromSpec = (value) => {
    if (value && typeof value === 'object') {
        const source = String(value.source ?? '').toLowerCase();
        if (source === 'file' || value.path) return { mode: 'file', value: String(value.path ?? '').trim() };
        if (['user_name', 'user', 'manual', 'custom'].includes(source) || value.user_name) {
            return { mode: 'user', value: String(value.user_name ?? value.user ?? value.name ?? '').trim() };
        }
        return { mode: 'name', value: String(value.actor_name ?? value.name ?? '').trim() };
    }
    const text = String(value ?? '').trim();
    return { mode: isLinkFilePath(text) ? 'file' : 'name', value: text };
};
const isPackPath = (value) => /\.pack(\.zs)?$/i.test(String(value ?? '').trim());
/** Form donor entry for a spec string: files by extension, catalog armor as list picks, the rest typed. */
const donorFromSpec = (value, catalogActors = new Set()) => {
    const text = String(value ?? '').trim();
    const mode = isPackPath(text) ? 'file' : (catalogActors.has(text) ? 'list' : 'name');
    return { mode, value: text };
};
/** Actor name of a spec `sound` / `effect` entry: a string, or the tagged `{source, actor_name | path}` form. */
const linkDonor = (value) => {
    if (value && typeof value === 'object') return String(value.actor_name ?? value.name ?? value.path ?? '').trim();
    return String(value ?? '').trim();
};
/** XLink keys of the advanced effect: comma or newline separated, trimmed, without blanks or repeats. */
const effectKeyList = (value) => {
    const keys = [];
    for (const key of String(value ?? '').split(/[,\n]/).map((entry) => entry.trim())) {
        if (key && !keys.includes(key)) keys.push(key);
    }
    return keys;
};
/** Physics donor strings of a form or spec (string, string list or {mode, value} list) without blanks. */
const physicsDonors = (value) => (Array.isArray(value) ? value : [value])
    .map((entry) => String((entry && typeof entry === 'object' ? entry.value : entry) ?? '').trim())
    .filter(Boolean);
/** Ingredients one Great Fairy step may ask for (vanilla rows never use more). */
const MAX_MATERIALS = 3;
const emptyMaterial = () => ({ actor: '', count: '' });
/** One rank: defense, rupees and up to three ingredient rows (the first one is required). */
const emptyUpgrade = () => ({ defense: '', rupees: '', materials: [emptyMaterial()] });
/**
 * Spec / template materials (`{actor, count}`, `{name, number}` or
 * `[actor, count]`) as form rows; actors vanilla rows spell differently for
 * the same pouch item (Item_Mushroom_D) become the catalog's entry
 * (Item_MushroomGet_D). Always at least one row, at most three.
 */
const materialsFromSpec = (materials, aliases = {}) => {
    const rows = (materials || [])
        .map((material) => {
            const actor = String(Array.isArray(material) ? material[0] : (material?.actor ?? material?.name ?? '')).trim();
            const count = Array.isArray(material) ? material[1] : (material?.count ?? material?.number ?? 1);
            return { actor: aliases[actor] || actor, count: String(count ?? 1) };
        })
        .filter((row) => row.actor)
        .slice(0, MAX_MATERIALS);
    return rows.length ? rows : [emptyMaterial()];
};
/** Form rows as spec materials: blank rows are dropped, a blank count means one. */
const materialsToSpec = (materials) => (materials || [])
    .filter((row) => row.actor)
    .map((row) => ({ actor: row.actor, count: Math.max(1, toInt(row.count) ?? 1) }));

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
        if (form.audio.trim()) spec.sound = form.audio.trim();
        const effect = linkToSpec(form.effect);
        if (effect !== undefined) spec.effect = effect;
        if (form.effectAi && effectKeyList(form.effectKeys).length) spec.effect_keys = effectKeyList(form.effectKeys);
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
                materials: materialsToSpec(upgrade.materials),
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
    if (form.indestructible) parameters.indestructible = true;
    else if (toInt(form.maxLife) !== undefined) parameters.max_life = toInt(form.maxLife);
    if (toInt(form.additionalDamage) !== undefined) parameters.additional_damage = toInt(form.additionalDamage);
    if (form.kind === 'Shield' && toInt(form.shieldBashDamage) !== undefined) parameters.shield_bash_damage = toInt(form.shieldBashDamage);
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
        ...(form.audio.trim() ? { sound: form.audio.trim() } : {}),
        ...(linkToSpec(form.effect) !== undefined ? { effect: linkToSpec(form.effect) } : {}),
        ...(form.effectAi && effectKeyList(form.effectKeys).length ? { effect_keys: effectKeyList(form.effectKeys) } : {}),
        ...(form.fbx && form.replaceBones ? { replace_bones: true } : {}),
        weapon_parameters: parameters,
        assets,
        vendors,
    };
}

/** Inverse of buildSpec, for the Edit button. */
function formFromSpec(spec, ingredientAliases = {}, catalogActors = new Set()) {
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
    form.audio = linkDonor(spec.sound ?? spec.audio ?? spec.sound_actor);
    form.effect = linkFromSpec(spec.effect ?? spec.effect_actor);
    const effectKeys = effectKeyList((spec.effect_keys ?? spec.xlink_keys ?? spec.effect_ai_keys ?? []).join(','));
    form.effectAi = effectKeys.length > 0;
    form.effectKeys = effectKeys.join(', ');
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
            materials: materialsFromSpec(upgrade.materials || upgrade.items, ingredientAliases),
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
        form.indestructible = Boolean(spec.weapon_parameters?.indestructible);
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
    const [generateRstb, setGenerateRstb] = useState(false);
    /** ELink entry of the mod list being edited on the Add ELink tab, if any. */
    const [elinkEditing, setElinkEditing] = useState(null);
    const [tab, setTab] = useState('weapon');
    const [form, setForm] = useState(() => emptyForm('weapon'));
    const [placeholders, setPlaceholders] = useState({ name: '', description: '' });
    const [templateUpgrades, setTemplateUpgrades] = useState([]);
    const [hideUpgrades, setHideUpgrades] = useState(true);
    const [items, setItems] = useState([]);
    const [fillingKeys, setFillingKeys] = useState(false);
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
    // Great Fairy ingredients: every pouch item a vanilla upgrade asks for,
    // once each, shown through the template picker with their own icons.
    const ingredients = catalog?.ingredients || [];
    const ingredientTemplates = useMemo(() => ingredients.map((ingredient) => ({
        actor: ingredient.actor,
        name: ingredient.name || ingredient.actor,
        kind: 'ingredient',
        upgraded: false,
        decayed: false,
    })), [ingredients]);
    // Other actors vanilla rows use for the same pouch item -> the listed actor.
    const ingredientAliases = useMemo(() => Object.fromEntries(ingredients.flatMap((ingredient) =>
        (ingredient.aliases || []).map((alias) => [alias, ingredient.actor]))), [ingredients]);
    const [ingredientIcons, setIngredientIcons] = useState({});
    const ingredientIconsRequested = useRef(false);
    const ensureIngredientIcons = useCallback(() => {
        if (ingredientIconsRequested.current || ingredients.length === 0) return;
        ingredientIconsRequested.current = true;
        const iconActors = [...new Set(ingredients.map((ingredient) => ingredient.iconActor || ingredient.actor))];
        invoke('item_creator_ingredient_icons', { names: iconActors })
            .then((result) => setIngredientIcons(Object.fromEntries(ingredients
                .map((ingredient) => [ingredient.actor, result[ingredient.iconActor || ingredient.actor]])
                .filter(([, icon]) => icon))))
            .catch(() => { ingredientIconsRequested.current = false; });
    }, [ingredients]);
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
        const names = [...new Set(items.map((item) => item.template_actor))].filter((name) => name && !icons[name]);
        if (names.length === 0) return;
        invoke('item_creator_icons', { names })
            .then((result) => setIcons((current) => ({ ...current, ...result })))
            .catch(() => {});
    }, [items, icons]);

    const update = (patch) => setForm((current) => ({ ...current, ...patch }));

    const suggestActorName = useCallback((kindId, currentTab, list) => {
        const taken = new Set([...(catalog?.templates || []).map((template) => template.actor), ...list.map((item) => item.actor_name).filter(Boolean)]);
        if (currentTab === 'armor') {
            const slot = ARMOR_SLOTS.find((entry) => entry.id === kindId) || ARMOR_SLOTS[0];
            const existing = list.find((item) => item.actor_name?.startsWith('Armor_'));
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
        setElinkEditing(null);
        setFormError('');
        setPlaceholders({ name: '', description: '' });
        // The ELink tab has no item form of its own.
        if (nextTab === 'elink') return;
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
    const updateMaterial = (rank, index, patch) => updateUpgrade(rank, {
        materials: form.upgrades[rank].materials.map((material, position) => position === index ? { ...material, ...patch } : material),
    });
    // Chosen ingredients need their icons even before a picker is opened
    // (a spec loaded for editing, the template's chain).
    useEffect(() => {
        if (tab === 'armor' && form.upgrades.some((upgrade) => upgrade.materials.some((material) => material.actor))) ensureIngredientIcons();
    }, [tab, form.upgrades, ensureIngredientIcons]);
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
    const pickEffectFile = async () => {
        const selected = await open({ multiple: false, directory: false, filters: [{ name: 'ELink parameter or actor pack', extensions: ['bgyml', 'byml', 'pack', 'zs'] }] });
        if (typeof selected === 'string') update({ effect: { ...form.effect, value: selected } });
    };

    const validate = () => {
        const actor = form.actorName.trim();
        if (!form.template) return 'Select a template.';
        if (!actor) return 'Enter an actor ID.';
        if (!/^[A-Za-z0-9_]+$/.test(actor)) return 'The actor ID may only contain letters, digits and underscores.';
        const effectValue = String(form.effect?.value ?? '').trim();
        if (form.effect?.mode === 'user' && effectValue && !/^[A-Za-z0-9_]+$/.test(effectValue)) return 'The effect user name may only contain letters, digits and underscores.';
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
        if (form.effectAi && effectKeyList(form.effectKeys).length === 0) return 'Advanced effect: enter at least one XLink key, or untick the box.';
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
                const chosen = upgrade.materials.filter((material) => material.actor);
                if (chosen.length === 0) return `Rank ${rank}: choose at least the first ingredient.`;
                if (new Set(chosen.map((material) => material.actor)).size !== chosen.length) return `Rank ${rank}: each ingredient can be listed only once.`;
                for (const material of chosen) {
                    const count = toInt(material.count);
                    if (String(material.count).trim() !== '' && (count === undefined || count < 1)) return `Rank ${rank}: the count of ${ingredients.find((entry) => entry.actor === material.actor)?.name || material.actor} must be at least one.`;
                }
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

    /** Puts an ELink request from the Add ELink tab into the mod list (replacing the entry being edited). */
    const commitElink = (spec) => {
        const replacing = editingIndex >= 0 && editingIndex < items.length;
        setItems((current) => {
            const next = [...current];
            if (replacing) next[editingIndex] = spec; else next.push(spec);
            return next;
        });
        setStatus({ kind: '', text: `ELink ${spec.newName} ${replacing ? 'updated' : 'added'}. ${replacing ? items.length : items.length + 1} entr${(replacing ? items.length : items.length + 1) === 1 ? 'y' : 'ies'} in the mod.` });
        setEditingIndex(-1);
        setElinkEditing(null);
    };
    const cancelElinkEdit = () => { setEditingIndex(-1); setElinkEditing(null); };

    const editItem = () => {
        const spec = items[selectedIndex];
        if (!spec) return;
        if (isElinkSpec(spec)) {
            setTab('elink');
            setElinkEditing(spec);
            setEditingIndex(selectedIndex);
            setFormError('');
            return;
        }
        setElinkEditing(null);
        const loaded = formFromSpec(spec, ingredientAliases, physicsTemplateActors);
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
        if (editingIndex === selectedIndex) { setEditingIndex(-1); setElinkEditing(null); }
        setSelectedIndex(-1);
    };
    const clearItems = () => { setItems([]); setSelectedIndex(-1); setEditingIndex(-1); setElinkEditing(null); };

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
            const valid = list.filter((spec) => spec && ((typeof spec.actor_name === 'string' && typeof spec.template_actor === 'string') || isElinkSpec(spec)));
            setItems(valid);
            setSelectedIndex(-1);
            setEditingIndex(-1);
            setElinkEditing(null);
            const vendorName = valid.find((spec) => spec.vendors?.[0]?.actor_name)?.vendors[0].actor_name;
            if (vendorName) setVendor(vendorName);
            // The spec file names the mod and places it: `C:\mods\MyCape.json`
            // -> mod `MyCape` written next to the file, in `C:\mods`.
            const parts = path.split(/[\\/]/);
            const stem = parts.pop().replace(/\.[^.]*$/, '');
            if (stem) setModName(stem);
            const parent = parts.join('\\');
            if (parent) setOutputDir(parent);
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
                generateRstb,
            });
            const rstbText = result.rstbEntries == null ? 'RSTB skipped' : `RSTB ${result.rstbEntries} entries`;
            const summary = `Mod ${modName} created in ${result.seconds.toFixed(1)} s: ${result.weapons.length} weapon(s), ${result.armors.length} armor piece(s), ${result.elinks.length} ELink(s), ${rstbText}.\n${result.outputRomfs}`
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

    const effectPlaceholder = {
        name: 'Vanilla actor whose Component/ELink effect file is copied, e.g. Item_Weapon_01 (optional)',
        file: 'ELinkParam .bgyml or actor .pack(.zs) whose ELink file is used as it is',
        user: 'ELink user name, e.g. Item_Weapon_01_custom made on the Add ELink tab',
    };
    // "From ELink entries": the effect AI keys of the calls edited in the
    // ELink entry of this mod that the Effect (User name) points at.
    const effectElinkSpec = form.effectAi && form.effect.mode === 'user'
        ? items.find((item) => isElinkSpec(item) && elinkName(item) === form.effect.value.trim()) || null
        : null;
    const fillEffectKeysFromElink = async () => {
        if (!effectElinkSpec) return;
        setFillingKeys(true);
        try {
            const entries = await invoke('elink_user_assets', { user: elinkBase(effectElinkSpec) });
            const edits = (effectElinkSpec.entries || []).filter((edit) => edit && Object.keys(edit.params || {}).length > 0);
            const keys = [];
            for (const edit of edits) {
                const entry = entries.find((candidate) => candidate.id === edit.id);
                if (!entry || !entry.key) continue;
                const twins = entries.filter((candidate) => candidate.id !== entry.id && candidate.key === entry.key);
                if (twins.length) {
                    setStatus({ kind: 'error', text: `Entry ${entry.id} (${entry.path}) cannot be emitted by name: ${elinkBase(effectElinkSpec)} also has ${twins.map((twin) => `${twin.id} (${twin.path})`).join(', ')} named ${entry.key}. Edit an entry with a unique name instead (its emitter set can be changed to the same one).` });
                    return;
                }
                if (!keys.includes(entry.key)) keys.push(entry.key);
            }
            if (!keys.length) {
                setStatus({ kind: 'error', text: `No edited asset call in the ELink entry ${elinkName(effectElinkSpec)}: edit the calls to emit (e.g. give them a Bone) first.` });
            } else {
                update({ effectKeys: keys.join(', ') });
                setStatus({ kind: '', text: `XLink keys taken from ${elinkName(effectElinkSpec)}: ${keys.join(', ')}` });
            }
        } catch (reason) {
            setStatus({ kind: 'error', text: `Could not read the ELink entries: ${reason}` });
        } finally {
            setFillingKeys(false);
        }
    };
    const effectLink = <div className="item-creator-link-entry">
        <select value={form.effect.mode} onChange={(event) => update({ effect: { mode: event.target.value, value: '' } })}>
            {LINK_MODES.map((mode) => <option key={mode.id} value={mode.id}>{mode.label}</option>)}
        </select>
        <input type="text" value={form.effect.value} placeholder={effectPlaceholder[form.effect.mode]} onChange={(event) => update({ effect: { ...form.effect, value: event.target.value } })} />
        <button type="button" hidden={form.effect.mode !== 'file'} title="Choose a file" onClick={pickEffectFile}>…</button>
    </div>;
    const effectHint = "Effect: Actor name copies a vanilla actor's ELink file; File takes an ELinkParam .bgyml or an actor pack's ELink entry as it is (a modded pack keeps its custom user); User name writes a fresh ELinkParam naming that ELink user (e.g. one created with Add ELink, present in the same mod). The ActorParam ELinkRef binds it and the RSDB ActorInfo ELinkUserName follows.";

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
            <label htmlFor="item-creator-rstb">RSTB</label>
            <div className="item-creator-check">
                <input id="item-creator-rstb" type="checkbox" checked={generateRstb} onChange={(event) => setGenerateRstb(event.target.checked)} />
                <label htmlFor="item-creator-rstb" className="item-creator-hint">Generate RSTB (off by default: TKMM rebuilds the ResourceSizeTable itself; tick it for a mod installed without a merger)</label>
            </div>
            {/* <label htmlFor="item-creator-zstd">RSTB zstd level</label>
            <input id="item-creator-zstd" type="number" min="1" max="22" placeholder="default" value={zstdLevel} onChange={(event) => setZstdLevel(event.target.value)} /> */}
        </div>

        <div className="item-creator-body">
            <div className="item-creator-main">
                <div className="item-creator-tabs">
                    <button type="button" className={tab === 'weapon' ? 'active' : ''} onClick={() => switchTab('weapon')}>weapon</button>
                    <button type="button" className={tab === 'armor' ? 'active' : ''} onClick={() => switchTab('armor')}>armor</button>
                    <button type="button" className={tab === 'elink' ? 'active' : ''} onClick={() => switchTab('elink')}>ELink</button>
                </div>
                <div className={`item-creator-panel${tab === 'elink' ? ' elink' : ''}`}>
                    <AddElinkPanel
                        active={tab === 'elink'}
                        editing={elinkEditing}
                        takenNames={items.filter((item, index) => isElinkSpec(item) && index !== editingIndex).map(elinkName)}
                        onCommit={commitElink}
                        onCancelEdit={cancelElinkEdit} />
                    {tab !== 'elink' && <>
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
                            <label>Indestructible</label>
                            <div className="item-creator-check">
                                <input id="item-creator-indestructible" type="checkbox" checked={form.indestructible} onChange={(event) => update({ indestructible: event.target.checked })} />
                                <label htmlFor="item-creator-indestructible" className="item-creator-hint">Never loses durability or breaks (LifeParam InitInvincibilityType: InvincibleNoDamageReaction)</label>
                            </div>
                            {!form.indestructible && <>
                                <label>Durability</label>
                                <input type="number" value={form.maxLife} onChange={(event) => update({ maxLife: event.target.value })} />
                            </>}
                            {!isBow && <>
                                <label>Fuse damage</label>
                                <input type="number" value={form.additionalDamage} onChange={(event) => update({ additionalDamage: event.target.value })} />
                            </>}
                            {isShield && <>
                                <label>Shield bash damage</label>
                                <input type="number" value={form.shieldBashDamage} onChange={(event) => update({ shieldBashDamage: event.target.value })} />
                            </>}
                            <label>Physics</label>
                            <input type="text" value={form.physics} placeholder="Vanilla actor whose Phive / Physics files are copied (optional)" onChange={(event) => update({ physics: event.target.value })} />
                            <label>Audio</label>
                            <input type="text" value={form.audio} placeholder="Vanilla actor whose Component/SLink sound file is copied (optional)" onChange={(event) => update({ audio: event.target.value })} />
                            <label>Effect</label>
                            {effectLink}
                            <span className="item-creator-hint">The donor's SLink / ELink file replaces the template's and the ActorParam SLinkRef / ELinkRef binds it. A blank name or one without an actor pack (or that component) in the RomFS is skipped. {effectHint}</span>
                            <label>Advanced effect</label>
                            <div className="item-creator-check">
                                <input id="item-creator-effect-ai" type="checkbox" checked={form.effectAi} onChange={(event) => update({ effectAi: event.target.checked })} />
                                <label htmlFor="item-creator-effect-ai" className="item-creator-hint">Add a root AI that emits effects of the ELink user at spawn (permanent glow, trail, …)</label>
                            </div>
                            {form.effectAi && <>
                                <label>XLink keys</label>
                                <div className="item-creator-keys-row">
                                    <input type="text" value={form.effectKeys} placeholder="Asset-call names of the effect donor's ELink user, comma separated, e.g. 古代矢完成, 軌跡 — or auto" onChange={(event) => update({ effectKeys: event.target.value })} />
                                    <button type="button" disabled={!effectElinkSpec || fillingKeys} title={effectElinkSpec ? `Keys of the calls edited in the ELink entry ${elinkName(effectElinkSpec)}` : 'Available when the Effect is a User name matching an ELink entry of this mod'} onClick={fillEffectKeysFromElink}>From ELink entries</button>
                                </div>
                                <span className="item-creator-hint">Writes AI/&lt;actor&gt;_Effect.root.ainb (one OneShotXLinkSearchAndEmit per key) plus its AIInfo and binds them with the ActorParam AIInfoRef. The keys are entries of the ELink user the weapon ends up with (the Effect donor's, e.g. Item_Weapon_01's 古代矢完成 / 軌跡 / 射撃), as listed under that user in ELink2/elink2.Product.*.belnk.zs; the word auto stands for the calls edited in the ELink entry the Effect names. Looping entries keep running, so the effect is permanent.</span>
                            </>}
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
                                    placeholder="Keep the template's material visibility"
                                    noneLabel="None (keep the template's lists)" />
                                <span className="item-creator-hint">Copies the chosen actor's ArmorParam material visibility into the new piece: HiddenMaterialGroupList (the body-skin materials the piece hides) and HideMaterialGroupNameList (the body-model material groups it covers, e.g. G_UpperBeltSet, G_Head). A list the donor lacks is dropped so the default applies, as on the donor.</span>
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
                            <label>Audio</label>
                            <div className="item-creator-skin-material">
                                <input type="text" value={form.audio} placeholder="Vanilla actor whose Component/SLink sound file is copied, e.g. Armor_014_Upper (optional)" onChange={(event) => update({ audio: event.target.value })} />
                                <span className="item-creator-hint">Copies the donor's SLink file into the piece, binds it with the ActorParam SLinkRef and takes the ArmorParam sound keys (SoundMaterial, HasSoundCloth, IsBarefootSound) from the donor. A blank name or one without an actor pack (or SLink) in the RomFS is skipped.</span>
                            </div>
                            <label>Effect</label>
                            <div className="item-creator-skin-material">
                                {effectLink}
                                <span className="item-creator-hint">{effectHint} A vanilla donor also lends its ArmorParam wind-effect keys (WindEffectMesh, WindEffectScale); a blank name or one without an actor pack (or ELink) in the RomFS is skipped.</span>
                            </div>
                            <label>Advanced effect</label>
                            <div className="item-creator-check">
                                <input id="item-creator-effect-ai" type="checkbox" checked={form.effectAi} onChange={(event) => update({ effectAi: event.target.checked })} />
                                <label htmlFor="item-creator-effect-ai" className="item-creator-hint">Add a root AI that emits effects of the ELink user at spawn (permanent glow, gloom, …)</label>
                            </div>
                            {form.effectAi && <label>XLink keys</label>}
                            {form.effectAi && <div className="item-creator-skin-material">
                                <div className="item-creator-keys-row">
                                    <input type="text" value={form.effectKeys} placeholder="Asset-call names of the effect donor's ELink user, comma separated, e.g. Miasma_Status_In — or auto" onChange={(event) => update({ effectKeys: event.target.value })} />
                                    <button type="button" disabled={!effectElinkSpec || fillingKeys} title={effectElinkSpec ? `Keys of the calls edited in the ELink entry ${elinkName(effectElinkSpec)}` : 'Available when the Effect is a User name matching an ELink entry of this mod'} onClick={fillEffectKeysFromElink}>From ELink entries</button>
                                </div>
                                <span className="item-creator-hint">Writes AI/&lt;actor&gt;_Effect.root.ainb (one OneShotXLinkSearchAndEmit per key) plus its AIInfo and binds them with the ActorParam AIInfoRef; upgrade ranks inherit it. The keys are entries of the ELink user the piece ends up with (the Effect donor's, e.g. Player's Miasma_Status_In), as listed under that user in ELink2/elink2.Product.*.belnk.zs; the word auto stands for the calls edited in the ELink entry the Effect names. Looping entries keep running, so the effect is permanent.</span>
                            </div>}
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
                                        <div className="item-creator-upgrade-head">
                                            <span className="item-creator-upgrade-rank">{'★'.repeat(index + 1)}</span>
                                            <input type="number" value={upgrade.defense} placeholder="Defense" title="Defense at this rank"
                                                onChange={(event) => updateUpgrade(index, { defense: event.target.value })} />
                                            <input type="number" value={upgrade.rupees} placeholder="Rupees" title="Rupees the Great Fairy charges for this step"
                                                onChange={(event) => updateUpgrade(index, { rupees: event.target.value })} />
                                            <span className="item-creator-hint">Ingredients (first required, up to {MAX_MATERIALS})</span>
                                            <button type="button" title="Remove this rank and the ones after it" onClick={() => update({ upgrades: form.upgrades.slice(0, index) })}>×</button>
                                        </div>
                                        {upgrade.materials.map((material, slot) => (
                                            <div className="item-creator-material-entry" key={slot}>
                                                <TemplatePicker
                                                    templates={ingredientTemplates}
                                                    value={material.actor}
                                                    icons={ingredientIcons}
                                                    onOpen={ensureIngredientIcons}
                                                    onChange={(actor) => updateMaterial(index, slot, { actor })}
                                                    placeholder={slot === 0 ? 'Choose an ingredient' : 'None (optional)'}
                                                    noneLabel={slot === 0 ? null : 'None'} />
                                                <input type="number" min="1" value={material.count} placeholder="Count" title="How many the Great Fairy asks for"
                                                    disabled={!material.actor} onChange={(event) => updateMaterial(index, slot, { count: event.target.value })} />
                                                <button type="button" title="Remove this ingredient" disabled={upgrade.materials.length <= 1}
                                                    onClick={() => updateUpgrade(index, { materials: upgrade.materials.filter((_, position) => position !== slot) })}>−</button>
                                                <button type="button" title="Add another ingredient" hidden={slot !== upgrade.materials.length - 1 || upgrade.materials.length >= MAX_MATERIALS}
                                                    disabled={!material.actor}
                                                    onClick={() => updateUpgrade(index, { materials: [...upgrade.materials, emptyMaterial()] })}>+</button>
                                            </div>
                                        ))}
                                    </div>
                                ))}
                                <div className="item-creator-upgrade-actions">
                                    <button type="button" disabled={form.upgrades.length >= MAX_UPGRADES} onClick={() => update({ upgrades: [...form.upgrades, emptyUpgrade()] })}>Add rank</button>
                                    <button type="button" disabled={templateUpgrades.length === 0} title="Copy the template's Great Fairy chain (defense, rupees and materials per rank)"
                                        onClick={() => update({ upgrades: templateUpgrades.slice(0, MAX_UPGRADES).map((entry) => ({ defense: entry.defense ?? '', rupees: entry.rupees ?? '', materials: materialsFromSpec(entry.materials, ingredientAliases) })) })}>Use template's upgrades</button>
                                    <span className="item-creator-hint">{form.upgrades.length ? `${form.upgrades.length} custom rank(s) (the rest of the chain is dropped). The ingredient list holds every item a vanilla Great Fairy upgrade asks for.` : 'Empty: the four ranks are derived from the template.'}</span>
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
                    </>}
                </div>
            </div>

            <div className="item-creator-content">
                <h2>Mod content</h2>
                <div className="item-creator-list">
                    {items.length === 0 && <div className="item-creator-empty">No items yet.</div>}
                    {items.map((item, index) => <div
                        key={`${isElinkSpec(item) ? elinkName(item) : item.actor_name}-${index}`}
                        className={`item-creator-item${index === selectedIndex ? ' selected' : ''}`}
                        onClick={() => setSelectedIndex(index)}
                        onDoubleClick={() => { setSelectedIndex(index); setTimeout(editItem, 0); }}>
                        <img src={isElinkSpec(item) ? ELINK_ICON : (icons[item.template_actor] || BLANK_ICON)} alt="" />
                        <span className="item-creator-option-name">
                            <span>{isElinkSpec(item) ? elinkName(item) : item.actor_name}</span>
                            <small>{isElinkSpec(item) ? `ELink effect from ${elinkBase(item)}` : item.display_name}</small>
                        </span>
                    </div>)}
                </div>
                <div className="item-creator-content-actions">
                    <button type="button" disabled={selectedIndex < 0} onClick={editItem}>Edit</button>
                    <button type="button" disabled={selectedIndex < 0} onClick={removeItem}>Remove from mod</button>
                    <button type="button" disabled={items.length === 0} onClick={clearItems}>Clear list</button>
                    <button type="button" disabled={items.length === 0} onClick={exportSpecs}>Export JSON…</button>
                    <button type="button" onClick={importSpecs}>Import JSON…</button>
                    <button type="button" className="item-creator-primary" disabled={generating || items.length === 0 || !catalog} onClick={createMod}>
                        {generating ? 'Creating mod…' : 'Create mod'}
                    </button>
                </div>
            </div>
        </div>

        <footer className="item-creator-footer">
            <div className={`item-creator-status ${status.kind}`}>{status.text}</div>
        </footer>
    </section>;
}

export default ItemCreator;
