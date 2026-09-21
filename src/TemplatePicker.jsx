import { useEffect, useRef, useState } from 'react';

/** Placeholder shown for templates without a cached icon. */
export const BLANK_ICON = 'menu/blank.webp';

export const templateLabel = (template) => `${template.name || template.actor}${template.decayed ? ' (decayed)' : ''}`;

/**
 * Drop-down of vanilla templates with their icons (shared by the Item
 * creator's weapon, armor and Zonai forms and by its ingredient pickers).
 */
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
        <button type="button" className="item-creator-picker-button" disabled={disabled} onClick={() => { setIsOpen((open) => !open); if (!isOpen && onOpen) onOpen(); }}>
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

export default TemplatePicker;
