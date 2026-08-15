import * as monaco from 'monaco-editor';
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import {
    activateDocument, addCleanDocument, closeDocument, getDocumentsSnapshot,
    subscribeDocuments,
} from './DocumentState';
import { useEditorContext } from './StateManager';

const emptySnapshot = () => ({
    activeTab: 'SARC', statusText: 'Ready', selectedPath: { path: '', isfile: false },
    labelTextDisplay: { sarc: '', yaml: '', rstb: '', comparer: '' },
    paths: { paths: [], added_paths: [], modded_paths: [], nested_paths: {}, file_type: '', root_name: '' },
    searchInSarcQuery: '',
    treeFilterQuery: '',
    pathsFilters: { showAll: true, showAdded: false, showModded: false },
    treeExpandedNodes: new Set(),
    compareData: { decision: 'FilesFromDisk', content1: '', content2: '', filepath1: '', filepath2: '', isSmall: true, isFromDisk: false, isInternal: false, label1: '', label2: '', isTiedToMonaco: false, lang: 'yaml' },
    editorText: '', editorLanguage: 'yaml', readOnly: false,
});

const initialSnapshotForDocument = (document) => document?.modelPaths?.length
    ? { ...emptySnapshot(), activeTab: '3D', readOnly: true }
    : emptySnapshot();

const modelUri = (id) => `inmemory://totkbits/${id}`;
const isUsableModel = (model) => Boolean(model && !model.isDisposed());
const isOwnedModel = (model, id) => isUsableModel(model) && model.uri.toString() === modelUri(id);

const snapshotFor = (context, model) => ({
    activeTab: context.activeTab, statusText: context.statusText,
    selectedPath: context.selectedPath, labelTextDisplay: context.labelTextDisplay,
    paths: context.paths, pathsFilters: context.pathsFilters,
    searchInSarcQuery: context.searchInSarcQuery,
    treeFilterQuery: context.treeFilterQuery,
    treeExpandedNodes: new Set(context.treeExpandedNodes), compareData: context.compareData,
    editorText: isUsableModel(model) ? model.getValue() : '',
    editorLanguage: isUsableModel(model) ? model.getLanguageId() : 'yaml', readOnly: context.readOnly || false,
});

export default function DocumentTabs() {
    const { documents, activeDocumentId } = useSyncExternalStore(subscribeDocuments, getDocumentsSnapshot);
    const context = useEditorContext();
    const contextRef = useRef(context);
    const previousDocumentIdRef = useRef(activeDocumentId);
    const tabsRef = useRef(null);
    const [tabsOverflow, setTabsOverflow] = useState(false);
    const [contextMenu, setContextMenu] = useState(null);
    contextRef.current = context;

    useEffect(() => {
        if (context.rightDocumentId && !documents.some((document) => document.id === context.rightDocumentId)) {
            context.setRightDocumentId(null);
        }
    }, [documents, context.rightDocumentId]);

    useEffect(() => {
        if (!contextMenu) return undefined;
        const dismiss = () => setContextMenu(null);
        window.addEventListener('mousedown', dismiss);
        window.addEventListener('resize', dismiss);
        return () => {
            window.removeEventListener('mousedown', dismiss);
            window.removeEventListener('resize', dismiss);
        };
    }, [contextMenu]);

    useLayoutEffect(() => {
        const tabs = tabsRef.current;
        if (!tabs) return undefined;

        const updateOverflow = () => {
            const overflowing = tabs.scrollWidth > tabs.clientWidth + 1;
            setTabsOverflow((current) => current === overflowing ? current : overflowing);
            document.documentElement.style.setProperty('--document-tabs-h', overflowing ? '36px' : '28px');
        };

        updateOverflow();
        const observer = new ResizeObserver(updateOverflow);
        observer.observe(tabs);
        window.addEventListener('resize', updateOverflow);
        return () => {
            observer.disconnect();
            window.removeEventListener('resize', updateOverflow);
            document.documentElement.style.setProperty('--document-tabs-h', '28px');
        };
    }, [documents]);

    useLayoutEffect(() => {
        const latest = contextRef.current;
        const previous = previousDocumentIdRef.current;
        const isInitialDocument = previous === activeDocumentId
            && !latest.documentSnapshots.current.has(activeDocumentId);

        if (isInitialDocument) {
            const initialModel = latest.editorRef.current?.getModel();
            latest.documentSnapshots.current.set(activeDocumentId, snapshotFor(latest, initialModel));
            if (isOwnedModel(initialModel, activeDocumentId)) {
                latest.documentModels.current.set(activeDocumentId, initialModel);
            }
            return;
        }

        const previousModel = latest.editorRef.current?.getModel();
        const previousIsOpen = documents.some((document) => document.id === previous);
        if (previousIsOpen) {
            latest.documentSnapshots.current.set(previous, snapshotFor(latest, previousModel));
        }
        if (latest.editorRef.current) {
            if (previousIsOpen && isOwnedModel(previousModel, previous)) {
                latest.documentModels.current.set(previous, previousModel);
                const previousViewState = latest.editorRef.current.saveViewState();
                if (previousViewState) latest.documentViewStates.current.set(previous, previousViewState);
            }
            const activeDocument = documents.find((document) => document.id === activeDocumentId);
            const snapshot = latest.documentSnapshots.current.get(activeDocumentId)
                || initialSnapshotForDocument(activeDocument);
            let model = latest.documentModels.current.get(activeDocumentId);
            if (!isOwnedModel(model, activeDocumentId)) {
                latest.documentModels.current.delete(activeDocumentId);
                const existing = monaco.editor.getModel(monaco.Uri.parse(modelUri(activeDocumentId)));
                if (isOwnedModel(existing, activeDocumentId)) existing.dispose();
                model = monaco.editor.createModel(
                    snapshot.editorText || '',
                    snapshot.editorLanguage || 'yaml',
                    monaco.Uri.parse(modelUri(activeDocumentId)),
                );
                latest.documentModels.current.set(activeDocumentId, model);
            }
            if (isUsableModel(model)) latest.editorRef.current.setModel(model);
            latest.editorRef.current.updateOptions({ readOnly: snapshot.readOnly || false, domReadOnly: snapshot.readOnly || false });
            const viewState = latest.documentViewStates.current.get(activeDocumentId);
            if (viewState && isUsableModel(model)) latest.editorRef.current.restoreViewState(viewState);
        }
        const activeDocument = documents.find((document) => document.id === activeDocumentId);
        const snapshot = latest.documentSnapshots.current.get(activeDocumentId)
            || initialSnapshotForDocument(activeDocument);
        latest.setActiveTab(snapshot.activeTab);
        latest.setStatusText('Ready');
        latest.setSelectedPath(snapshot.selectedPath);
        latest.setLabelTextDisplay(snapshot.labelTextDisplay);
        latest.setpaths(snapshot.paths);
        latest.setSearchInSarcQuery(snapshot.searchInSarcQuery || '');
        latest.setTreeFilterQuery(snapshot.treeFilterQuery || '');
        latest.setPathsFilters(snapshot.pathsFilters || { showAll: true, showAdded: false, showModded: false });
        latest.setTreeExpandedNodes(new Set(snapshot.treeExpandedNodes || []));
        latest.setCompareData(snapshot.compareData);
        latest.setReadOnly(snapshot.readOnly || false);
        previousDocumentIdRef.current = activeDocumentId;
    }, [activeDocumentId]);

    const handleClose = async (event, id) => {
        event.stopPropagation();
        const wasActive = id === activeDocumentId;
        if (wasActive) {
            const latest = contextRef.current;
            const currentModel = latest.editorRef.current?.getModel();
            latest.documentSnapshots.current.set(id, snapshotFor(latest, currentModel));
            if (isOwnedModel(currentModel, id)) {
                latest.documentModels.current.set(id, currentModel);
                const viewState = latest.editorRef.current.saveViewState();
                if (viewState) latest.documentViewStates.current.set(id, viewState);
            }
        }
        await closeDocument(id);
        const model = context.documentModels.current.get(id);
        const cleanup = () => {
            const attachedModel = context.editorRef.current?.getModel();
            if (isOwnedModel(model, id) && attachedModel !== model) model.dispose();
            context.documentModels.current.delete(id);
            context.documentViewStates.current.delete(id);
            context.documentSnapshots.current.delete(id);
        };
        if (wasActive) requestAnimationFrame(cleanup);
        else cleanup();
    };

    useEffect(() => {
        const handleCloseActiveDocument = () => {
            if (activeDocumentId) {
                void handleClose({ stopPropagation: () => { } }, activeDocumentId);
            }
        };
        window.addEventListener('totkbits:close-active-document', handleCloseActiveDocument);
        return () => {
            window.removeEventListener('totkbits:close-active-document', handleCloseActiveDocument);
        };
    }, [activeDocumentId]);

    const handleSwitchToParent = (event) => {
        event.stopPropagation();
        const child = documents.find((document) => document.id === contextMenu?.documentId);
        const parentIsOpen = child?.parentDocumentId
            && documents.some((document) => document.id === child.parentDocumentId);
        setContextMenu(null);
        if (parentIsOpen) activateDocument(child.parentDocumentId);
        else contextRef.current.setStatusText('ERROR: parent file was closed');
    };

    const captureActiveDocument = () => {
        const latest = contextRef.current;
        const model = latest.editorRef.current?.getModel();
        latest.documentSnapshots.current.set(activeDocumentId, snapshotFor(latest, model));
        if (isOwnedModel(model, activeDocumentId)) latest.documentModels.current.set(activeDocumentId, model);
    };

    const moveToRightView = (event) => {
        event.stopPropagation();
        const id = contextMenu?.documentId;
        if (!id) return;
        captureActiveDocument();
        contextRef.current.setRightDocumentId(id);
        if (id === activeDocumentId) {
            const replacement = documents.find((document) => document.id !== id);
            if (replacement) activateDocument(replacement.id);
            else addCleanDocument();
        }
        setContextMenu(null);
    };

    const moveToLeftView = (event) => {
        event.stopPropagation();
        const id = contextMenu?.documentId;
        if (!id) return;
        if (contextRef.current.rightDocumentId === id) contextRef.current.setRightDocumentId(null);
        activateDocument(id);
        setContextMenu(null);
    };

    // if (documents.length == 1 && documents[0].title == "Untitled") {
    if (documents.length == 0 || (documents.length == 1 && documents[0].clean)) {
        // console.log(documents[0]);
        return null;
    }

    return <><div
        ref={tabsRef}
        className={`document-tabs ${tabsOverflow ? 'is-overflowing' : ''}`}
        role="tablist"
    >
        {documents.map((document) => <button
            type="button" role="tab" aria-selected={document.id === activeDocumentId}
            title={document.fullPath || document.title}
            className={`document-tab ${document.id === activeDocumentId ? 'active' : ''}`}
            key={document.id} onClick={() => {
                if (contextRef.current.rightDocumentId === document.id) {
                    contextRef.current.setRightDocumentId(null);
                }
                activateDocument(document.id);
            }}
            onContextMenu={(event) => {
                event.preventDefault();
                event.stopPropagation();
                activateDocument(document.id);
                setContextMenu({ documentId: document.id, x: event.clientX, y: event.clientY });
            }}
            onMouseDown={(event) => {
                if (event.button === 1) {
                    event.preventDefault();
                    void handleClose(event, document.id);
                }
            }}
        >
            <span>{document.title}</span>
            <span className="document-tab-close" onClick={(event) => handleClose(event, document.id)}>×</span>
        </button>)}
        <button type="button" className="document-tab-add" title="New document" onClick={addCleanDocument}>+</button>
    </div>
        {contextMenu && createPortal(<div
            className="document-tab-context-menu"
            style={{ left: contextMenu.x, top: contextMenu.y }}
            onMouseDown={(event) => event.stopPropagation()}
        >
            {/* <button type="button" onClick={moveToLeftView}>Move to left view</button>
            <button type="button" onClick={moveToRightView}>Move to right view</button> */}
            {documents.find((document) => document.id === contextMenu.documentId)?.parentDocumentId
                && <button type="button" onClick={handleSwitchToParent}>Switch to parent</button>}
        </div>, document.body)}
    </>;
}
