import { invoke as tauriInvoke } from '@tauri-apps/api/core';

const documentCommands = new Set([
    'add_empty_byml_file', 'extract_opened_sarc', 'extract_folder_from_opened_sarc',
    'load_tomodachi_texture_set',
    'get_toml_config', 'update_toml_config', 'edit_config', 'open_file_struct',
    'open_file_from_path', 'open_folder_struct', 'edit_internal_file', 'save_file_struct', 'save_as_click',
    'add_click', 'add_to_dir_click', 'add_files_from_dir_recursively',
    'extract_internal_file', 'rename_internal_sarc_file', 'remove_internal_sarc_file',
    'rstb_get_entries', 'rstb_edit_entry', 'rstb_remove_entry', 'search_in_sarc',
    'clear_search_in_sarc', 'compare_files', 'compare_internal_file_with_vanila',
    'close_all_opened_files',
    'expand_nested_sarc', 'edit_nested_sarc_file', 'open_bphcl_leaf', 'extract_nested_sarc_file',
    'remove_bphcl_node',
    'mutate_nested_archive',
    'open_bfwav_node', 'replace_bfwav_node', 'replace_bars_audio_from_folder', 'export_bfwav_node', 'open_amta_node',
    'inspect_3d_model', 'export_g1m_fbx', 'replace_g1m_meshes', 'list_batch_render_files',
    'render_image', 'export_image_png', 'rename_bntx_texture', 'replace_bntx_image',
]);

const createDocument = (title = 'Untitled') => ({
    id: crypto.randomUUID(), title, fullPath: '', fileMetadata: '', fileType: 'NONE', parentDocumentId: null, clean: true,
});
let documents = [createDocument()];
let activeDocumentId = documents[0].id;
let snapshot = { documents, activeDocumentId };
const listeners = new Set();
const comparisonSources = new Map();

const emit = () => {
    snapshot = { documents, activeDocumentId };
    listeners.forEach((listener) => listener());
};
export const subscribeDocuments = (listener) => { listeners.add(listener); return () => listeners.delete(listener); };
export const getDocumentsSnapshot = () => snapshot;
export const getActiveDocumentId = () => activeDocumentId;
export const activateDocument = (id) => {
    if (id === activeDocumentId || !documents.some((document) => document.id === id)) return;
    activeDocumentId = id;
    emit();
};

export const addCleanDocument = () => {
    const document = createDocument();
    documents = [...documents, document];
    activateDocument(document.id);
    emit();
    return document.id;
};

export const openUtilityDocument = (title, utilityTab) => {
    const existing = documents.find((document) => document.utilityTab === utilityTab);
    if (existing) {
        activateDocument(existing.id);
        return { id: existing.id, created: false };
    }
    const reusable = documents.length === 1 && documents[0].clean ? documents[0] : null;
    const id = reusable ? reusable.id : addCleanDocument();
    documents = documents.map((document) => document.id === id
        ? { ...document, title, clean: false, utilityTab, fileType: 'OTHER' }
        : document);
    emit();
    return { id, created: true };
};

export const openModelCollectionDocument = (paths, title = 'Selected AOC models') => {
    const modelPaths = [...new Set(paths.filter(Boolean))].slice(0, 5);
    if (modelPaths.length < 2) return null;
    const reusable = documents.length === 1 && documents[0].clean ? documents[0] : null;
    const id = reusable ? reusable.id : addCleanDocument();
    documents = documents.map((document) => document.id === id
        ? {
            ...document,
            title,
            fullPath: `aoc-models://${modelPaths.join('|')}`,
            fileMetadata: '[G1M] [ReadOnly]',
            fileType: 'G1M',
            modelPaths,
            clean: false,
        }
        : document);
    activateDocument(id);
    emit();
    return id;
};

const updateTitle = (id, title, opened = false) => {
    documents = documents.map((document) => document.id === id
        ? { ...document, title: title || document.title, clean: opened ? false : document.clean }
        : document);
    emit();
};

const updateFullPath = (id, fullPath) => {
    documents = documents.map((document) => document.id === id
        ? { ...document, fullPath: fullPath || document.fullPath }
        : document);
    emit();
};

const updateFileMetadata = (id, fileMetadata) => {
    documents = documents.map((document) => document.id === id
        ? { ...document, fileMetadata: fileMetadata || '' }
        : document);
    emit();
};

const updateFileType = (id, fileType) => {
    documents = documents.map((document) => document.id === id
        ? { ...document, fileType: fileType || 'NONE' }
        : document);
    emit();
};

export const closeDocument = async (id) => {
    const closingDocument = documents.find((document) => document.id === id);
    await tauriInvoke('close_document', { documentId: id });
    if (closingDocument) {
        window.dispatchEvent(new CustomEvent('totkbits:model-cache-purge', {
            detail: {
                paths: closingDocument.modelPaths?.length
                    ? closingDocument.modelPaths
                    : [closingDocument.fullPath].filter(Boolean),
            },
        }));
    }
    const wasActive = id === activeDocumentId;
    documents = documents.filter((document) => document.id !== id);
    comparisonSources.delete(id);
    if (documents.length === 0) documents = [createDocument()];
    if (wasActive) activateDocument(documents[documents.length - 1].id);
    emit();
};

export const discardActiveComparisonDocument = async () => {
    const comparisonDocumentId = activeDocumentId;
    const sourceDocumentId = comparisonSources.get(comparisonDocumentId);
    if (!sourceDocumentId) return;
    await closeDocument(comparisonDocumentId);
    activateDocument(sourceDocumentId);
};

const allocateOpenDocument = (command, args) => {
    const title = command === 'open_file_from_path' && args?.path
        ? args.path.replace(/\\/g, '/').split('/').pop()
        : 'Opening…';
    const reusable = documents.length === 1 && documents[0].clean ? documents[0] : null;
    const id = reusable ? reusable.id : addCleanDocument();
    if (reusable && id !== activeDocumentId) activateDocument(id);
    updateTitle(id, title);
    return id;
};

const allocateChildDocument = (args, parentDocumentId) => {
    const rawPath = args?.innerPath || args?.path || 'Archive entry';
    const title = rawPath.replace(/\\/g, '/').split('/').pop();
    const id = addCleanDocument();
    updateTitle(id, title);
    documents = documents.map((document) => document.id === id
        ? { ...document, parentDocumentId }
        : document);
    emit();
    return id;
};

export const invoke = async (command, args = {}) => {
    if (!documentCommands.has(command)) return tauriInvoke(command, args);
    const isOpen = command === 'open_file_struct' || command === 'open_file_from_path' || command === 'open_folder_struct';
    const isChildOpen = command === 'edit_internal_file' || command === 'edit_nested_sarc_file'
        || command === 'open_bphcl_leaf' || command === 'open_amta_node';
    const isComparison = command === 'compare_files'
        || command === 'compare_internal_file_with_vanila'
        || (command === 'mutate_nested_archive' && args?.action === 'compare');
    const sourceDocumentId = activeDocumentId;
    const parentDocumentId = isChildOpen
        ? (args?.parentDocumentId || activeDocumentId)
        : null;
    const comparisonDocumentId = isComparison ? addCleanDocument() : null;
    if (comparisonDocumentId) {
        comparisonSources.set(comparisonDocumentId, sourceDocumentId);
        updateTitle(comparisonDocumentId, 'Comparison');
        updateFileType(comparisonDocumentId, 'OTHER');
    }
    const documentId = isComparison ? sourceDocumentId
        : isOpen ? allocateOpenDocument(command, args)
            : isChildOpen ? allocateChildDocument(args, parentDocumentId) : activeDocumentId;
    const openOperationId = (isOpen || isChildOpen)
        ? `open:${documentId}:${crypto.randomUUID()}`
        : null;
    if (openOperationId) {
        const name = (args?.innerPath || args?.path)?.replace(/\\/g, '/').split('/').pop();
        window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
            detail: { id: openOperationId, label: name ? `Opening ${name}…` : 'Opening file…' },
        }));
    }

    // Fast parsers (notably plain text and small YAML-backed formats) can return
    // before React has attached Monaco to the newly allocated document. Wait for
    // that activation commit so the open handler writes into the correct model.
    if (isOpen || isChildOpen || isComparison) {
        await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    if (isComparison) {
        const sourceName = documents.find((document) => document.id === sourceDocumentId)?.title || 'file';
        window.dispatchEvent(new CustomEvent('totkbits:comparing', { detail: sourceName }));
        await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    try {
        const result = await tauriInvoke(command, {
            ...args, documentId,
            ...(isChildOpen ? { parentDocumentId } : {}),
        });
        if (isOpen || isChildOpen) {
            const failed = !result || result.tab === 'ERROR';
            if (failed) {
                await closeDocument(documentId);
                return null;
            }
            else {
                const g1mTitle = result.file_metadata?.includes('[G1M]')
                    ? result.file_label?.replace(/\s+\[G1M\]$/, '')
                    : null;
                updateTitle(documentId, g1mTitle || result.path?.name || result.file_label?.split(' [')[0] || 'Document', true);
                updateFullPath(documentId, result.path?.full_path || args?.innerPath || args?.path || '');
                updateFileMetadata(documentId, result.file_metadata || '');
                updateFileType(documentId, result.file_type);
                if (isOpen && command !== 'open_folder_struct') {
                    tauriInvoke('get_recent_files').then((recentFiles) => {
                        window.dispatchEvent(new CustomEvent('totkbits:recent-files-changed', {
                            detail: recentFiles,
                        }));
                    }).catch(() => null);
                }
                // Existing open handlers update the visible pane after invoke resolves.
                // Re-select the originating document so those updates cannot overwrite
                // a different document if the user switched while opening.
                activateDocument(documentId);
            }
        }
        if (isComparison) {
            const failed = !result || result.tab === 'ERROR'
                || result.status_text?.toLowerCase().startsWith('error:');
            if (failed) {
                await closeDocument(comparisonDocumentId);
                activateDocument(sourceDocumentId);
                return result;
            }
            const firstLabel = result.compare_data?.file1?.label || 'Comparison';
            const title = `Compare: ${firstLabel.replace(/\\/g, '/').split('/').pop()}`;
            updateTitle(comparisonDocumentId, title, true);
            activateDocument(comparisonDocumentId);
        }
        if (command === 'save_as_click' && result?.tab !== 'ERROR' && result?.path?.full_path) {
            updateTitle(documentId, result.path.name || 'BARS', true);
            updateFullPath(documentId, result.path.full_path);
        }
        if (command === 'close_all_opened_files') {
            const oldDocuments = documents;
            documents = [createDocument()];
            activeDocumentId = documents[0].id;
            emit();
            await Promise.all(oldDocuments.map((document) => tauriInvoke('close_document', { documentId: document.id }).catch(() => null)));
        }
        return result;
    } catch (error) {
        if (isOpen || isChildOpen) await closeDocument(documentId);
        if (isComparison && comparisonDocumentId) {
            await closeDocument(comparisonDocumentId);
            activateDocument(sourceDocumentId);
        }
        throw error;
    } finally {
        if (openOperationId) {
            window.dispatchEvent(new CustomEvent('totkbits:model-loading', {
                detail: { id: openOperationId, done: true },
            }));
        }
        if (isComparison) {
            window.dispatchEvent(new CustomEvent('totkbits:comparing', { detail: '' }));
        }
    }
};
