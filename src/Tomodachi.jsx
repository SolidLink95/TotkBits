import { useEffect, useRef } from 'react';
import { invoke } from './DocumentState';

const textureMarkers = {
    'Base color': ['_alb', 'albedo'],
    Normal: ['_nrm', 'normal'],
    Roughness: ['_mic', '_rgh', 'roughness'],
    Specular: ['_mic', '_spc', 'specular'],
    Metalness: ['_mic', '_mtl', 'metalness'],
    Emission: ['_emm', 'emission', 'emissive'],
    Mask: ['_msk', '_alp', 'mask'],
};

export function findTomodachiTexture(textures, type) {
    const markers = textureMarkers[type];
    if (!markers) return null;
    return Object.entries(textures).find(([name]) => {
        const lower = name.toLowerCase();
        return !lower.includes('::last') && markers.some((marker) => lower.includes(marker));
    })?.[1] || null;
}

export default function Tomodachi({ model, setModel, documentPath, setStatusText }) {
    const requestRef = useRef(0);
    const selectedSet = model?.tomodachiTextureSets?.find((set) => set.name === model.tomodachiTextureSet);
    const hasSetChoices = (model?.tomodachiTextureSets?.length || 0) > 1;
    const hasVariantChoices = (selectedSet?.variants?.length || 0) > 1;
    const showSetDropdown = hasSetChoices || hasVariantChoices;

    useEffect(() => {
        requestRef.current += 1;
    }, [documentPath]);

    const selectTextures = async (textureSet, textureVariant) => {
        if (!documentPath) return;
        const requestId = requestRef.current + 1;
        requestRef.current = requestId;
        setModel((current) => current ? {
            ...current,
            tomodachiTextureSet: textureSet,
            tomodachiTextureVariant: textureVariant,
        } : current);
        const variantLabel = String(textureVariant).padStart(2, '0');
        setStatusText(`Loading texture set ${textureSet}.${variantLabel}…`);
        try {
            const textures = await invoke('load_tomodachi_texture_set', {
                path: documentPath,
                textureSet,
                textureVariant,
            });
            if (requestId !== requestRef.current) return;
            setModel((current) => current ? {
                ...current,
                tomodachiTextureSet: textureSet,
                tomodachiTextureVariant: textureVariant,
                resolvedTextures: [
                    ...(current.resolvedTextures || []).filter((texture) => texture.source !== 'tomodachi'),
                    ...(textures || []),
                ],
            } : current);
            setStatusText(`Texture set ${textureSet}.${variantLabel} loaded`);
        } catch (reason) {
            if (requestId === requestRef.current) {
                setStatusText(`Unable to load texture set ${textureSet}: ${reason}`);
            }
        }
    };

    if (!hasSetChoices && !hasVariantChoices) return null;

    return <div style={{ padding: '12px 0 0 12px' }}>
        <label><strong>Tomodachi</strong></label>
        <div className="bfres-export-panel">
            {showSetDropdown && <label>Textures:
                <select value={model.tomodachiTextureSet || ''} onChange={(event) => {
                    const set = model.tomodachiTextureSets.find((entry) => entry.name === event.target.value);
                    selectTextures(set.name, set.variants[0]);
                }}>
                    {model.tomodachiTextureSets.map((set) => <option value={set.name} key={set.name}>{set.name}</option>)}
                </select>
            </label>}
            {hasVariantChoices && <label>Number:
                <select value={model.tomodachiTextureVariant ?? selectedSet.variants[0]}
                    onChange={(event) => selectTextures(model.tomodachiTextureSet, Number(event.target.value))}>
                    {selectedSet.variants.map((variant) => <option value={variant} key={variant}>.{String(variant).padStart(2, '0')}</option>)}
                </select>
            </label>}
        </div>
    </div>;
}
