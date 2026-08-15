import React, { useEffect, useState } from 'react';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { invoke } from './DocumentState';
import './Mii.css';

export function useMiiName(activeTab, document) {
    const [name, setName] = useState('');
    useEffect(() => {
        setName('');
        if (activeTab !== 'IMAGE' || document?.fileType !== 'MII') return;
        let cancelled = false;
        invoke('read_mii_name', { documentId: document.id })
            .then((value) => { if (!cancelled) setName(value); })
            .catch(() => { if (!cancelled) setName(''); });
        return () => { cancelled = true; };
    }, [activeTab, document?.id, document?.fileType, document?.fullPath]);
    return name;
}

export function MiiRendererCredit() {
    return <section className="mii-renderer-credit">
        <p>Mii Renderer created by Arian Kordi: <a href="https://mii-unsecure.ariankordi.net/" onClick={(event) => {
            event.preventDefault();
            openExternal(event.currentTarget.href);
        }}>https://mii-unsecure.ariankordi.net/</a></p>
    </section>;
}

export const isMiiDocument = (document) => document?.fileType === 'MII';

export const miiDisplayName = (document, name, fallback) => isMiiDocument(document) && name
    ? name
    : fallback;

async function convertMiiWithMiiJs(path) {
    const { default: MiiJS } = await import('miijs/browser');
    const encoded = await invoke('read_file_base64', { path });
    const input = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0));
    const charInfoEx = extractTomodachiCharInfoEx(input);
    const mii = charInfoEx
        ? await MiiJS.Mii.create(convertCharInfoExToMii(charInfoEx))
        : await MiiJS.Mii.create(input);
    const fileStem = path.replace(/\\/g, '/').split('/').pop().replace(/\.[^.]+$/, '');
    const decodedName = String(mii.get('name') || '').trim();
    const miiName = (decodedName || fileStem.replace(/_/g, ' ').trim()).slice(0, 10) || 'Mii';
    mii.set('name', miiName);
    const converted = mii.encode(MiiJS.MiiFormats.RCD);
    let binary = '';
    for (let offset = 0; offset < converted.length; offset += 0x8000) {
        binary += String.fromCharCode(...converted.subarray(offset, offset + 0x8000));
    }
    return { data: btoa(binary), miiName };
}

function extractTomodachiCharInfoEx(input) {
    if (input.length === 152) return input;
    if (input.length === 156 && new DataView(input.buffer, input.byteOffset, input.byteLength).getUint32(0, true) === 152) {
        return input.subarray(4);
    }
    // `.ltd` v1-v3: four-byte share header, then a length-prefixed 152-byte Mii block.
    if (input.length >= 160 && input[0] >= 1 && input[0] <= 3) {
        const view = new DataView(input.buffer, input.byteOffset, input.byteLength);
        if (view.getUint32(4, true) === 152) return input.subarray(8, 160);
    }
    return null;
}

const clamp = (value, minimum, maximum) => Math.min(maximum, Math.max(minimum, value));

function convertCharInfoExToMii(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let offset = 0;
    const u8 = () => view.getUint8(offset++);
    const u16 = () => { const value = view.getUint16(offset, true); offset += 2; return value; };
    const u32 = () => { const value = view.getUint32(offset, true); offset += 4; return value; };
    const wideName = () => {
        const units = [];
        for (let index = 0; index < 11; index++) units.push(u16());
        const end = units.indexOf(0);
        return String.fromCharCode(...units.slice(0, end < 0 ? units.length : end)).trim();
    };

    const createId = [u32(), u32(), u32(), u32()]
        .map((value) => value.toString(16).padStart(8, '0')).join('').toUpperCase();
    const name = wideName().slice(0, 10) || 'Mii';
    u8(); // fontRegion
    const gender = u8();
    const height = u8();
    const build = u8();
    u8(); // regionMove
    const faceFlags = u8();
    const faceType = u8();
    const faceColor = u8();
    for (let index = 0; index < 10; index++) u8(); // two wrinkle layers
    for (let index = 0; index < 12; index++) u8(); // two makeup layers
    const hairType = u16();
    const hairColor = u8();
    u8(); u8(); u8();
    const hairStyle = u8();
    u8(); u8(); u8(); // ears
    const eyeType = u8();
    const eyeColor = u8();
    const eyeScale = u8();
    const eyeAspect = u8();
    const eyeRotate = u8();
    const eyeX = u8();
    const eyeY = u8();
    for (let index = 0; index < 13; index++) u8(); // shadow, highlight, upper eyelash
    for (let index = 0; index < 18; index++) u8(); // lower eyelash and both eyelids
    const eyebrowType = u8();
    const eyebrowColor = u8();
    const eyebrowScale = u8();
    const eyebrowAspect = u8();
    const eyebrowRotate = u8();
    const eyebrowX = u8();
    const eyebrowY = u8();
    const noseType = u8();
    const noseScale = u8();
    const noseY = u8();
    const mouthType = u8();
    const mouthColor = u8();
    const mouthScale = u8();
    const mouthAspect = u8();
    u8(); // mouthRotate has no conventional CharInfo equivalent
    const mouthY = u8();
    const beardType = u8();
    const beardColor = u8();
    const mustacheType = u8();
    u8(); // separate short-beard color
    const secondaryMustacheType = u8();
    const mustacheColor = u8();
    const mustacheScale = u8();
    u8(); // mustacheAspect
    const mustacheY = u8();
    const glassesType = u8();
    const glassesColor = u8();
    const glassesScale = u8();
    u8(); // glassesAspect
    const glassesY = u8();
    u8(); u8(); // secondary glasses
    const moleScale = u8();
    const moleX = u8();
    const moleY = u8();

    return {
        meta: { name, miiId: createId, type: 'Default', charset: 0, region: 0 },
        general: {
            favoriteColor: 0,
            gender: gender & 1,
            height: clamp(height, 0, 127),
            weight: clamp(build, 0, 127),
        },
        face: { type: clamp(faceType, 0, 11), color: clamp(faceColor, 0, 9), feature: 0, makeup: 0 },
        hair: { type: clamp(hairType, 0, 131), color: clamp(hairColor, 0, 99), flipped: (hairStyle & 1) !== 0 },
        eyes: {
            type: clamp(eyeType, 0, 59), color: clamp(eyeColor, 0, 99), size: clamp(eyeScale, 0, 7),
            squash: clamp(eyeAspect, 0, 6), rotation: clamp(eyeRotate, 0, 7),
            distanceApart: clamp(eyeX, 0, 12), yPosition: clamp(eyeY, 0, 18),
        },
        eyebrows: {
            type: clamp(eyebrowType, 0, 23), color: clamp(eyebrowColor, 0, 99), size: clamp(eyebrowScale, 0, 8),
            squash: clamp(eyebrowAspect, 0, 6), rotation: clamp(eyebrowRotate, 0, 11),
            distanceApart: clamp(eyebrowX, 0, 12), yPosition: clamp(eyebrowY, 0, 18),
        },
        nose: { type: clamp(noseType, 0, 17), size: clamp(noseScale, 0, 8), yPosition: clamp(noseY, 0, 18) },
        mouth: {
            type: clamp(mouthType, 0, 35), color: clamp(mouthColor, 0, 99), size: clamp(mouthScale, 0, 8),
            squash: clamp(mouthAspect, 0, 6), yPosition: clamp(mouthY, 0, 18),
        },
        beard: {
            type: clamp(beardType, 0, 5), color: clamp(beardColor, 0, 99),
            mustache: {
                type: clamp(secondaryMustacheType || mustacheType, 0, 5), color: clamp(mustacheColor, 0, 99),
                size: clamp(mustacheScale, 0, 8), yPosition: clamp(mustacheY, 0, 16),
            },
        },
        glasses: {
            type: clamp(glassesType, 0, 19), color: clamp(glassesColor, 0, 99),
            size: clamp(glassesScale, 0, 7), yPosition: clamp(glassesY, 0, 20),
        },
        mole: {
            on: (faceFlags & 0x80) !== 0, size: clamp(moleScale, 0, 8),
            xPosition: clamp(moleX, 0, 16), yPosition: clamp(moleY, 0, 30),
        },
    };
}

export async function addRflMii(internalPath, path, setStatusText) {
    setStatusText('Converting Mii to Wii Mii data…');
    const converted = await convertMiiWithMiiJs(path);
    return invoke('add_archive_bytes', {
        internalPath: `${internalPath}/${converted.miiName}.miigx`.replace(/^\//, ''),
        data: converted.data,
        overwrite: false,
    });
}

export async function replaceRflMii(internalPath, path, setStatusText) {
    setStatusText('Converting replacement Mii…');
    const converted = await convertMiiWithMiiJs(path);
    let content = await invoke('add_archive_bytes', { internalPath, data: converted.data, overwrite: true });
    if (content?.tab === 'ERROR') return content;
    const directory = internalPath.includes('/') ? internalPath.slice(0, internalPath.lastIndexOf('/') + 1) : '';
    const oldFile = internalPath.slice(directory.length);
    const slotPrefix = oldFile.match(/^\d{3}_/)?.[0] || '';
    const safeName = converted.miiName.replace(/[<>:"/\\|?*\x00-\x1F]/g, '_');
    const newInternalPath = `${directory}${slotPrefix}${safeName}.miigx`;
    if (newInternalPath !== internalPath) {
        content = await invoke('rename_internal_sarc_file', { internalPath, newInternalPath });
    }
    return content;
}

export async function clearRflMiis(setStatusText, setpaths) {
    const content = await invoke('clear_rfl_miis');
    if (!content) return;
    setStatusText(content.status_text);
    if (content.tab !== 'ERROR') setpaths(content.sarc_paths);
}

export async function downloadMiiGlb({ event, closeMenu, activeDocument, documents, setStatusText, openFile }) {
    event.stopPropagation();
    closeMenu();
    if (activeDocument?.fileType !== 'MII') return;
    const operationId = `mii-glb:${activeDocument.id}:${crypto.randomUUID()}`;
    window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
        detail: { id: operationId, label: `Downloading ${activeDocument.title || 'Mii'} GLB…` },
    }));
    try {
        setStatusText('Downloading Mii GLB...');
        await new Promise((resolve) => requestAnimationFrame(resolve));
        const parentDocument = documents.find((document) => document.id === activeDocument.parentDocumentId);
        const path = await tauriInvoke('download_mii_glb', {
            documentId: activeDocument.id,
            databasePath: parentDocument?.fullPath || null,
        });
        await openFile(path);
    } catch (error) {
        setStatusText(`Error downloading Mii GLB: ${String(error)}`);
    } finally {
        window.dispatchEvent(new CustomEvent('totkbits:model-loading', { detail: { id: operationId, done: true } }));
    }
}
