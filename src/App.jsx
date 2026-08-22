

import "./App.css";
import "./Comparer.css";
import { debounce } from "lodash"; // or any other method/utility to debounce
import React, { useEffect } from "react";
import * as monaco from "monaco-editor";
import { getDocumentsSnapshot, subscribeDocuments } from './DocumentState';
import { useSyncExternalStore } from 'react';
// import ReactDiffViewer from 'react-diff-viewer-continued';
import AddOrRenameFilePrompt from './AddOrRenameFilePrompt'; // Import the modal component
import ButtonsDisplay from "./Buttons";
import DirectoryTree from "./DirectoryTree";
import Comparer from "./Comparer";
import { useFileDropHandler } from './FileDropHandler';
import { MenuBarDisplayWithUpdateButton } from "./MenuBar";
import InitializeEditor from './MonacoEditor';
import RstbTree from "./RstbTree";
import { SearchTextInSarcPrompt } from './SearchTextInSarc';
import { useEditorContext } from './StateManager';
import { checkIfUpdateNeeded } from './ButtonClicks';
import  OptionsEditor  from './OptionsEditor';
import DocumentTabs from './DocumentTabs';
import PhysicsMerge from './PhysicsMerge';
import Bfres3DView from './Bfres3DView';
import AmtaView from './AmtaView';
import ImageView from './ImageView';
import AudioView from './AudioView';
import ModelBrowserView from './ModelBrowserView';


let triggered = false

function App() {
  const { documents } = useSyncExternalStore(subscribeDocuments, getDocumentsSnapshot);
  const [comparingFile, setComparingFile] = React.useState('');
  const [audioProcessing, setAudioProcessing] = React.useState('');
  const [loadingModel, setLoadingModel] = React.useState(null);

  useEffect(() => {
    const receive = (event) => setAudioProcessing(event.detail || '');
    window.addEventListener('totkbits:audio-processing', receive);
    return () => window.removeEventListener('totkbits:audio-processing', receive);
  }, []);

  useEffect(() => {
    const operations = new Map();
    const receive = (event) => {
      const { id, label, done } = event.detail || {};
      if (!id) return;
      if (done) operations.delete(id);
      else operations.set(id, { label: label || 'Loading model…', progress: event.detail?.progress });
      setLoadingModel(Array.from(operations.values()).at(-1) || null);
    };
    window.addEventListener('totkbits:model-loading', receive);
    return () => window.removeEventListener('totkbits:model-loading', receive);
  }, []);




  const {
    isOptionsOpen,
    settings, setSettings, updateState, setUpdateState,
    searchInSarcQuery, setSearchInSarcQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    physicsMergeReturnTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang, readOnly,
    documentModels, documentViewStates,
    rightEditorContainerRef, rightEditorRef, rightDocumentId, setRightDocumentId,
    splitRatio, setSplitRatio,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal, compareData,
    savingFile, documentSnapshots
  } = useEditorContext();

  const rightDocument = documents.find((document) => document.id === rightDocumentId);
  const rightSnapshot = rightDocumentId ? documentSnapshots.current.get(rightDocumentId) : null;

  useEffect(() => {
    if (!rightDocumentId || !rightEditorContainerRef.current) return undefined;
    const snapshot = documentSnapshots.current.get(rightDocumentId);
    if (!snapshot || snapshot.activeTab !== 'YAML') return undefined;
    let model = documentModels.current.get(rightDocumentId);
    if (!model || model.isDisposed()) {
      const uri = monaco.Uri.parse(`inmemory://totkbits/${rightDocumentId}`);
      model = monaco.editor.getModel(uri) || monaco.editor.createModel(
        snapshot.editorText || '', snapshot.editorLanguage || 'yaml', uri,
      );
      documentModels.current.set(rightDocumentId, model);
    }
    const editor = monaco.editor.create(rightEditorContainerRef.current, {
      model,
      theme: settings.theme,
      minimap: { enabled: settings.minimap },
      wordWrap: 'on',
      fontSize: settings.fontSize,
      readOnly: snapshot.readOnly || false,
      domReadOnly: snapshot.readOnly || false,
    });
    rightEditorRef.current = editor;
    const viewState = documentViewStates.current.get(rightDocumentId);
    if (viewState) editor.restoreViewState(viewState);
    const observer = new ResizeObserver(() => editor.layout());
    observer.observe(rightEditorContainerRef.current);
    return () => {
      const state = editor.saveViewState();
      if (state) documentViewStates.current.set(rightDocumentId, state);
      observer.disconnect();
      editor.dispose();
      if (rightEditorRef.current === editor) rightEditorRef.current = null;
    };
  }, [rightDocumentId, rightSnapshot?.activeTab, settings.theme, settings.minimap, settings.fontSize]);

  const startSplitDrag = (event) => {
    event.preventDefault();
    const move = (moveEvent) => setSplitRatio(Math.min(0.8, Math.max(0.2, moveEvent.clientX / window.innerWidth)));
    const stop = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', stop);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', stop);
  };
  
  const { isFileHovering, parsingFile } = useFileDropHandler(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);



  const handleNodeSelect = (path, isfile) => {
    setSelectedPath({ path, isfile });
    console.log(`Selected Node Path in App: ${path} isfile: ${isfile}`);
    // Here you can use selectedPath for any other logic in App.jsx
  };


  useEffect(() => {
    const handleComparing = (event) => setComparingFile(event.detail || '');
    window.addEventListener('totkbits:comparing', handleComparing);
    return () => window.removeEventListener('totkbits:comparing', handleComparing);
  }, []);

  useEffect(() => {
    // Initialize the Monaco editor only once
    if (!editorRef.current && editorContainerRef.current) {
      InitializeEditor({
        editorRef,
        editorContainerRef,
        editorValue,
        documentModels,
        // lang,
        setStatusText,
        setActiveTab,
        setLabelTextDisplay,
        setpaths,
        updateEditorContent,
        settings, setSettings,
      });
      if (!updateState.wasChecked) {
        checkIfUpdateNeeded(setUpdateState);
      }
    }

    // Function to update editor size, call it when needed
    const updateEditorSize = () => {
      if (editorRef.current && editorContainerRef.current) {
        const { width, height } = editorContainerRef.current.getBoundingClientRect();
        editorRef.current.layout({ width, height });
      }
    };

    // Call updateEditorSize immediately to ensure correct layout
    updateEditorSize();

    // Setup a debounced resize listener
    const debouncedUpdateEditorSize = debounce(updateEditorSize, 100);
    window.addEventListener("resize", debouncedUpdateEditorSize);

    return () => {
      window.removeEventListener("resize", debouncedUpdateEditorSize);
    };
  }, []); // Empty dependency array to run once on mount

  useEffect(() => {
    setStatusText('Ready');
  }, [activeTab, setStatusText]);

  useEffect(() => {
    // Directly adjust visibility without disposing the editor
    const editorDom = editorContainerRef.current;
    if (editorDom) {
      if (activeTab === 'YAML') {
        console.log(activeTab);
        editorDom.style.display = "block";
        // Ensure the editor is correctly sized each time the tab becomes active
        editorRef.current.layout();
      } else {
        editorDom.style.display = "none";
      }
    }
  }, [activeTab]);

  //Variables

  //Functions


  const statusStyle = {
    color: statusText.toLowerCase().startsWith("error") ? 'red' :
      statusText.toLowerCase().startsWith('warning') ? 'yellow' : 'white',
  };
  const isComparerWorking = (compareData?.content1?.length ?? 0) > 0;
  const displayButtons = activeTab === 'SARC' || activeTab === 'AUDIO' || activeTab === 'RSTB' || activeTab === 'YAML';
  const rootStyle = activeTab !== "COMPARER" ? {} : isComparerWorking ? {backgroundColor: "#2E303C"} : {};
  const uiScale = Math.min(3, Math.max(0.2, Number(settings.uiScale) || 1));
  return (
    <div
      className={`maincontainer ${rightDocumentId && activeTab !== '3D' && activeTab !== 'IMAGE' ? 'has-split-view' : ''}`}
      style={{
        ...rootStyle,
        '--buttons-h': displayButtons ? '33px' : '0px',
        '--split-position': `${splitRatio * 100}%`,
        zoom: uiScale,
        width: `${100 / uiScale}vw`,
        height: `${100 / uiScale}vh`,
      }}
    > 
      {isFileHovering && (
        <div className="file-drop-overlay" role="status">
          <div className="file-drop-message">Drop file to open</div>
        </div>
      )}
      {parsingFile && (
        <div className="parsing-overlay" role="status" aria-live="polite">
          <div className="parsing-content">
            <div className="loading-swirl" aria-hidden="true"></div>
            <div>Parsing {parsingFile}</div>
          </div>
        </div>
      )}
      {savingFile && (
        <div className="parsing-overlay" role="status" aria-live="polite">
          <div className="parsing-content">
            <div className="loading-swirl" aria-hidden="true"></div>
            <div>Saving {savingFile}</div>
          </div>
        </div>
      )}
      {!parsingFile && loadingModel && (
        <div className="parsing-overlay" role="status" aria-live="polite" aria-busy="true">
          <div className="parsing-content">
            <div className="loading-swirl" aria-hidden="true"></div>
            <div>{loadingModel.label}</div>
            {Number.isFinite(loadingModel.progress) && <progress className="batch-render-progress" max="100" value={loadingModel.progress} />}
          </div>
        </div>
      )}
      {audioProcessing && (
        <div className="parsing-overlay" role="status" aria-live="polite">
          <div className="parsing-content">
            <div className="loading-swirl" aria-hidden="true"></div>
            <div>{audioProcessing}</div>
          </div>
        </div>
      )}
      {comparingFile && (
        <div className="parsing-overlay" role="status" aria-live="polite">
          <div className="parsing-content">
            <div className="loading-swirl" aria-hidden="true"></div>
            <div>Comparing {comparingFile}</div>
          </div>
        </div>
      )}
      <MenuBarDisplayWithUpdateButton />
      <DocumentTabs />
      {/* {activeTab === 'LOADING' ? <div className="modal-overlay">Loading...</div> : null} */}
      <AddOrRenameFilePrompt
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        setStatusText={setStatusText}
        setpaths={setpaths}
        selectedPath={selectedPath}
        isAddPrompt={isAddPrompt}
        renamePromptMessage={renamePromptMessage}
      >
      </AddOrRenameFilePrompt>
      <OptionsEditor />
      <SearchTextInSarcPrompt
        setStatusText={setStatusText}
        setpaths={setpaths}
        searchInSarcQuery={searchInSarcQuery}
        setSearchInSarcQuery={setSearchInSarcQuery}
        isSearchInSarcOpened={isSearchInSarcOpened}
        documentSnapshots={documentSnapshots}
        setIsSearchInSarcOpened={setIsSearchInSarcOpened}>
      </SearchTextInSarcPrompt>

      {activeTab !== '3D' && activeTab !== 'IMAGE' && activeTab !== 'AMTA' && activeTab !== 'MODEL_BROWSER' && <ButtonsDisplay
        editorRef={editorRef}
        updateEditorContent={updateEditorContent}
        setStatusText={setStatusText}
        activeTab={activeTab}
        setActiveTab={setActiveTab}
        setLabelTextDisplay={setLabelTextDisplay}
        setpaths={setpaths}
        selectedPath={selectedPath}
        setIsModalOpen={setIsModalOpen}
        setIsAddPrompt={setIsAddPrompt}
      />}
      <DirectoryTree
        onNodeSelect={handleNodeSelect}
        sarcPaths={paths}
        //For buttons clicks
        setStatusText={setStatusText}
        activeTab={activeTab}
        style={{ display: activeTab === 'SARC' ? "block" : "none" }}
      />
      <RstbTree
        onNodeSelect={handleNodeSelect}
        sarcPaths={paths}
        //For buttons clicks
        setStatusText={setStatusText}
        activeTab={activeTab}
        style={{ display: activeTab === 'RSTB' ? "block" : "none" }}
      />
      <Comparer setStatusText={setStatusText} activeTab={activeTab}/>
      <PhysicsMerge activeTab={activeTab} setActiveTab={setActiveTab} returnTab={physicsMergeReturnTab} setStatusText={setStatusText} setpaths={setpaths} documentSnapshots={documentSnapshots} />
      <Bfres3DView activeTab={activeTab} setStatusText={setStatusText} />
      <ImageView activeTab={activeTab} setStatusText={setStatusText} />
      <AudioView activeTab={activeTab} setActiveTab={setActiveTab} setStatusText={setStatusText} setpaths={setpaths} />
      <AmtaView activeTab={activeTab} setActiveTab={setActiveTab} />
      <ModelBrowserView activeTab={activeTab} />
      

      {activeTab === 'YAML' && readOnly && <div className="physics-yaml-preview-banner" role="status">
        <strong>{labelTextDisplay.yaml?.includes('[Restbl]') || labelTextDisplay.yaml?.includes('[RSTB]')
          ? 'RSTB JSON preview'
          : labelTextDisplay.yaml?.includes('[HKCL]') ? 'HKCL YAML preview' : 'Physics YAML preview'}</strong>
        <span>Read-only parsed data</span>
      </div>}
      <div ref={editorContainerRef} className={`code_editor ${activeTab === 'YAML' && readOnly ? 'physics-yaml-preview-editor' : ''}`} style={{ display: activeTab === 'YAML' ? "block" : "none" }}></div>
      {rightDocumentId && activeTab !== '3D' && activeTab !== 'IMAGE' && <>
        <div className="split-view-divider" onMouseDown={startSplitDrag} role="separator" aria-orientation="vertical" />
        <section className="right-document-view">
          <header>
            <span>{rightDocument?.title || 'Right view'}</span>
            <button type="button" title="Close right view" onClick={() => setRightDocumentId(null)}>×</button>
          </header>
          {rightSnapshot?.activeTab === 'YAML'
            ? <div ref={rightEditorContainerRef} className="right-code-editor" />
            : <div className="right-document-summary">
                <strong>{rightSnapshot?.activeTab || 'Document'}</strong>
                {(rightSnapshot?.paths?.paths || []).map((path) => <div key={path}>{path}</div>)}
              </div>}
        </section>
      </>}
      {/* <div className="statusbar" style={statusStyle}>Current path: "{selectedPath.path} {selectedPath.endian}"</div> */}
      <div className="statusbar" style={statusStyle}>{statusText}</div>
    </div>
  );
}

export default App;
