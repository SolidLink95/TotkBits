import React, { useCallback, useEffect, useRef, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { invoke } from './DocumentState';
import { getDocumentsSnapshot, subscribeDocuments } from './DocumentState';
import { useSyncExternalStore } from 'react';
import './ImageView.css';
import { isMiiDocument, miiDisplayName, MiiRendererCredit, useMiiName } from './Mii';

const replacementFormats = [
    'A1_B5_G5_R5_UNORM', 'A4_B4_G4_R4_UNORM', 'B5_G5_R5_A1_UNORM', 'B5_G6_R5_UNORM',
    'B8_G8_R8_A8_SRGB', 'B8_G8_R8_A8_UNORM', 'R10_G10_B10_A2_UNORM', 'R16_UNORM',
    'R4_G4_B4_A4_UNORM', 'R4_G4_UNORM', 'R5_G5_B5_A1_UNORM', 'R5_G6_B5_UNORM',
    'R8_UNORM', 'R8_G8_UNORM', 'R8_G8_B8_A8_UNORM', 'R8_G8_B8_A8_SRGB',
    'BC1_SRGB', 'BC1_UNORM', 'BC2_SRGB', 'BC2_UNORM', 'BC3_SRGB', 'BC3_UNORM',
    'BC4_SNORM', 'BC4_UNORM', 'BC5_SNORM', 'BC5_UNORM', 'BC6_FLOAT', 'BC6_UFLOAT',
    'BC7_UNORM', 'BC7_SRGB',
];
const encodableFormats = new Set([
    'B8_G8_R8_A8_SRGB', 'B8_G8_R8_A8_UNORM', 'R8_UNORM', 'R8_G8_UNORM',
    'R8_G8_B8_A8_UNORM', 'R8_G8_B8_A8_SRGB', 'BC1_SRGB', 'BC1_UNORM',
    'BC2_SRGB', 'BC2_UNORM', 'BC3_SRGB', 'BC3_UNORM', 'BC4_SNORM', 'BC4_UNORM',
    'BC5_SNORM', 'BC5_UNORM', 'BC6_FLOAT', 'BC6_UFLOAT', 'BC7_UNORM', 'BC7_SRGB',
]);

export default function ImageView({ activeTab, setStatusText }) {
    const { documents, activeDocumentId } = useSyncExternalStore(subscribeDocuments, getDocumentsSnapshot);
    const document = documents.find((value) => value.id === activeDocumentId);
    const [image, setImage] = useState(null);
    const [error, setError] = useState('');
    const [zoom, setZoom] = useState(1);
    const [showTree, setShowTree] = useState(true);
    const [revision, setRevision] = useState(0);
    const [textureIndex, setTextureIndex] = useState(0);
    const [arrayIndex, setArrayIndex] = useState(0);
    const [mipIndex, setMipIndex] = useState(0);
    const [loading, setLoading] = useState(false);
    const [renameValue, setRenameValue] = useState('');
    const [replacementFormat, setReplacementFormat] = useState('ORIGINAL');
    const miiName = useMiiName(activeTab, document);
    const canvasRef = useRef(null);

    const fitImageToCanvas = useCallback(() => {
        const canvas = canvasRef.current;
        if (!canvas || !image?.width || !image?.height) return;
        const availableWidth = Math.max(1, canvas.clientWidth - 40);
        const availableHeight = Math.max(1, canvas.clientHeight - 40);
        const fitScale = Math.min(availableWidth / image.width, availableHeight / image.height);
        const fortyPercentScale = Math.min(
            availableWidth * 0.4 / image.width,
            availableHeight * 0.4 / image.height,
        );
        setZoom(Math.min(16, fitScale, Math.max(1, fortyPercentScale)));
    }, [image?.width, image?.height]);

    useEffect(() => {
        setTextureIndex(0);
        setArrayIndex(0);
        setMipIndex(0);
        setImage(null);
        setError('');
        setReplacementFormat('ORIGINAL');
    }, [document?.fullPath]);

    useEffect(() => {
        if (activeTab !== 'IMAGE' || !document?.fullPath) return;
        let cancelled = false;
        setLoading(true);
        setError('');
        invoke('render_image', { path: document.fullPath, textureIndex, arrayIndex, mipIndex }).then((result) => {
            if (cancelled) return;
            setImage(result);
            setRenameValue(result.entries?.[textureIndex]?.name || '');
        }).catch((reason) => {
            if (!cancelled) setError(String(reason));
        }).finally(() => {
            if (!cancelled) setLoading(false);
        });
        return () => { cancelled = true; };
    }, [activeTab, document?.fullPath, revision, textureIndex, arrayIndex, mipIndex]);

    useEffect(() => {
        if (activeTab !== 'IMAGE' || !image) return undefined;
        const frame = requestAnimationFrame(fitImageToCanvas);
        const observer = new ResizeObserver(fitImageToCanvas);
        if (canvasRef.current) observer.observe(canvasRef.current);
        return () => {
            cancelAnimationFrame(frame);
            observer.disconnect();
        };
    }, [activeTab, image, showTree, fitImageToCanvas]);

    if (activeTab !== 'IMAGE') return null;
    const exportPng = async () => {
        const output = await save({
            defaultPath: `${selectedEntry?.name || document?.title || 'image'}.png`,
            filters: [{ name: 'PNG image', extensions: ['png'] }],
        });
        if (!output) return;
        await invoke('export_image_png', { source: document.fullPath, output, textureIndex, arrayIndex, mipIndex });
        setStatusText(`Rendered PNG ${output}`);
    };
    const replaceDds = async () => {
        const png = await open({ multiple: false, filters: [{ name: 'PNG image', extensions: ['png'] }] });
        if (!png) return;
        await invoke('replace_dds_image', { target: document.fullPath, png, ddsType: image?.ddsType || '', mipCount: image?.mipCount || 1, textureIndex, arrayIndex, mipIndex, replacementFormat });
        setRevision((value) => value + 1);
        setStatusText(`Replaced ${document.title} from ${png}`);
    };
    const renameBntx = async () => {
        const nextName = renameValue.trim();
        if (!nextName || nextName === image?.entries?.[textureIndex]?.name) return;
        await invoke('rename_bntx_texture', { path: document.fullPath, textureIndex, newName: nextName });
        setRevision((value) => value + 1);
        setStatusText(`Renamed BNTX texture to ${nextName}`);
    };
    const replaceBntx = async () => {
        const png = await open({ multiple: false, filters: [{ name: 'PNG image', extensions: ['png'] }] });
        if (!png) return;
        await invoke('replace_bntx_image', { target: document.fullPath, png, textureIndex, arrayIndex, mipIndex, replacementFormat });
        setRevision((value) => value + 1);
        setStatusText(`Replaced ${selectedEntry?.name}, layer ${arrayIndex}, mip ${mipIndex}`);
    };
    const changeZoom = (amount) => setZoom((value) => Math.min(16, Math.max(0.05, value + amount)));
    const fileName = document?.fullPath?.replace(/\\/g, '/').split('/').pop() || document?.title || 'Image';
    const entries = image?.entries?.length ? image.entries : [{ name: fileName }];
    const selectedEntry = entries[textureIndex];
    const selectedDisplayName = miiDisplayName(document, miiName, selectedEntry?.name || fileName);
    const selectTexture = (index) => {
        if (index < 0 || index >= entries.length || index === textureIndex) return;
        setTextureIndex(index);
        setArrayIndex(0);
        setMipIndex(0);
    };
    const selectSubimage = (entry, subimage) => {
        setTextureIndex(entry);
        setArrayIndex(subimage.arrayIndex);
        setMipIndex(subimage.mipIndex);
    };

    const changeArray = (nextIndex) => {
        const count = selectedEntry?.arrayCount || 1;
        if (nextIndex < 0 || nextIndex >= count || loading) return;
        setArrayIndex(nextIndex);
    };
    const changeMip = (nextIndex) => {
        const count = selectedEntry?.mipCount || image?.mipCount || 1;
        if (nextIndex < 0 || nextIndex >= count || loading) return;
        setMipIndex(nextIndex);
    };
    const arrayImages = (entry) => {
        const surfaces = entry.subimages || [];
        const layers = surfaces.filter((surface) => surface.mipIndex === 0);
        return layers.length ? layers : [{
            arrayIndex: 0,
            mipIndex: 0,
            name: entry.name,
            width: entry.width,
            height: entry.height,
        }];
    };

    return <main className="image-workspace">
        <header>
            {/* <div><strong>{document?.title || 'Image'}</strong>{image && <span>{image.format} · {image.width} × {image.height}</span>}</div> */}
            {image && <span>{image.format} · {image.width} × {image.height}</span>}
            <button type="button" onClick={() => setShowTree((value) => !value)}>{showTree ? 'Hide tree' : 'Show tree'}</button>
            <div className="image-zoom-controls">
                <button type="button" onClick={() => changeZoom(-0.25)} aria-label="Zoom out">−</button>
                <button type="button" onClick={fitImageToCanvas} title="Fit image">{Math.round(zoom * 100)}%</button>
                <button type="button" onClick={() => changeZoom(0.25)} aria-label="Zoom in">+</button>
            </div>
            <button type="button" onClick={exportPng} disabled={!image}>Render PNG</button>
        </header>
        {showTree && <nav className="image-tree" aria-label="Image contents">
            <header>Images</header>
            <div className="image-tree-file"><span className="image-tree-caret">▾</span><span className="image-tree-file-icon">▣</span><strong title={fileName}>{fileName}</strong></div>
            <div className="image-tree-file-children">{entries.map((entry, index) => <div className="image-tree-entry" key={`${entry.name}-${index}`}>
                <button type="button" className="image-tree-container" onClick={() => selectTexture(index)} title={entry.name}>
                    <span className="image-tree-caret">▾</span><span className="image-tree-container-icon">▣</span><span>{entry.name}</span>
                </button>
                <div className="image-tree-children">{arrayImages(entry).map((subimage) => <button type="button" key={`${subimage.arrayIndex}`} className={textureIndex === index && arrayIndex === subimage.arrayIndex ? 'selected' : ''} onClick={() => selectSubimage(index, subimage)} title={subimage.name}>
                    <span className="image-tree-image-icon">▧</span><span className="image-tree-label">{isMiiDocument(document)
                        ? (miiName || fileName)
                        : (entry.arrayCount > 1 ? `${entry.name} [${subimage.arrayIndex}]` : entry.name)}</span><small>{subimage.width} × {subimage.height}</small>
                </button>)}</div>
            </div>)}</div>
        </nav>}
        <section className="image-center">
            <div className="image-surface-toolbar">
                <span>Array Level: {arrayIndex} / {Math.max(0, (selectedEntry?.arrayCount || 1) - 1)}</span>
                <button type="button" onClick={() => changeArray(arrayIndex - 1)} disabled={arrayIndex === 0 || loading} aria-label="Previous array level">‹</button>
                <button type="button" onClick={() => changeArray(arrayIndex + 1)} disabled={arrayIndex + 1 >= (selectedEntry?.arrayCount || 1) || loading} aria-label="Next array level">›</button>
                <span>Mip Level: {mipIndex} / {Math.max(0, (selectedEntry?.mipCount || image?.mipCount || 1) - 1)}</span>
                <button type="button" onClick={() => changeMip(mipIndex - 1)} disabled={mipIndex === 0 || loading} aria-label="Previous mip level">‹</button>
                <button type="button" onClick={() => changeMip(mipIndex + 1)} disabled={mipIndex + 1 >= (selectedEntry?.mipCount || image?.mipCount || 1) || loading} aria-label="Next mip level">›</button>
            </div>
            <div ref={canvasRef} className="image-canvas">
                {!image && !error && <span>Decoding image…</span>}
                {error && <div className="image-error">{error}</div>}
                {image && <img src={image.dataUrl} alt={document?.title || 'Decoded image'} style={{ width: `${image.width * zoom}px`, height: `${image.height * zoom}px` }} />}
                {loading && image && <span className="image-loading">Decoding {selectedEntry?.name || 'texture'}…</span>}
            </div>
        </section>
        <aside className="image-options">
            <header><strong>{selectedDisplayName}</strong><small>{image?.format || 'Image'}</small></header>
            {isMiiDocument(document) && <MiiRendererCredit />}
            {image && <dl><dt>Width</dt><dd>{image.width}</dd><dt>Height</dt><dd>{image.height}</dd><dt>Mip count</dt><dd>{image.mipCount}</dd>{image.ddsType && <><dt>DDS type</dt><dd>{image.ddsType}</dd></>}</dl>}
            {image?.entries?.[textureIndex] && <dl><dt>Array count</dt><dd>{image.entries[textureIndex].arrayCount}</dd><dt>Layer</dt><dd>{arrayIndex}</dd><dt>Mip level</dt><dd>{mipIndex}</dd><dt>Surface format</dt><dd>{image.entries[textureIndex].format}</dd></dl>}
            {(image?.format === 'DDS' || image?.format === 'G1T') && <section>
                <h3>Replace from PNG</h3>
                {/* <p>Replaces layer {arrayIndex}, mip {mipIndex} while preserving the file format and other surfaces.</p> */}
                {/* {image?.format === 'G1T'
                    ? <p>G1T format is preserved (for example {selectedEntry?.format || 'the original format'}).</p>
                    : <label>Output format<select value={replacementFormat} onChange={(event) => setReplacementFormat(event.target.value)}><option value="ORIGINAL">Keep original ({selectedEntry?.format})</option>{replacementFormats.map((format) => <option key={format} value={format} disabled={!encodableFormats.has(format)}>{format}{encodableFormats.has(format) ? '' : ' (unavailable)'}</option>)}</select></label>} */}
                <button type="button" onClick={replaceDds}>Replace</button>
            </section>}
            {image?.format === 'BNTX' && <section>
                <h3>Replace selected surface</h3>
                <p>Replaces layer {arrayIndex}, mip {mipIndex} while preserving the BNTX container.</p>
                <label>Output format<select value={replacementFormat} onChange={(event) => setReplacementFormat(event.target.value)}><option value="ORIGINAL">Keep original ({selectedEntry?.format})</option>{replacementFormats.map((format) => <option key={format} value={format} disabled={!encodableFormats.has(format)}>{format}{encodableFormats.has(format) ? '' : ' (unavailable)'}</option>)}</select></label>
                <button type="button" onClick={replaceBntx}>Replace</button>
                <h3>Rename texture</h3>
                <label>Texture name<input value={renameValue} onChange={(event) => setRenameValue(event.target.value)} /></label>
                <button type="button" onClick={renameBntx} disabled={!renameValue.trim() || renameValue.trim() === image.entries?.[textureIndex]?.name}>Rename</button>
                <small>Names must fit the BNTX texture's existing string slot.</small>
            </section>}
        </aside>
    </main>;
}
