

import "./App.css";
import "./Comparer.css";
import { debounce } from "lodash"; // or any other method/utility to debounce
import React, { useEffect } from "react";
// import ReactDiffViewer from 'react-diff-viewer-continued';
import ActiveTabDisplay from "./ActiveTab";
import AddOrRenameFilePrompt from './AddOrRenameFilePrompt'; // Import the modal component
import ButtonsDisplay from "./Buttons";
import DirectoryTree from "./DirectoryTree";
import Comparer from "./Comparer";
import { useFileDropHandler } from './FileDropHandler';
import MenuBarDisplay from "./MenuBar";
import InitializeEditor from './MonacoEditor';
import RstbTree from "./RstbTree";
import { SearchTextInSarcPrompt } from './SearchTextInSarc';
import { useEditorContext } from './StateManager';


let triggered = false

function App() {




  const {
    settings, setSettings,
    searchInSarcQuery, setSearchInSarcQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal, compareData
  } = useEditorContext();
  
  useFileDropHandler(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);



  const handleNodeSelect = (path, isfile) => {
    setSelectedPath({ path, isfile });
    console.log(`Selected Node Path in App: ${path} isfile: ${isfile}`);
    // Here you can use selectedPath for any other logic in App.jsx
  };


  useEffect(() => {
    // Initialize the Monaco editor only once
    if (!editorRef.current && editorContainerRef.current) {
      InitializeEditor({
        editorRef,
        editorContainerRef,
        editorValue,
        // lang,
        setStatusText,
        setActiveTab,
        setLabelTextDisplay,
        setpaths,
        updateEditorContent,
        settings, setSettings,
      });
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
  const isComparerWorking = compareData.content1.length >0;
  const displayButtons = activeTab === 'SARC' || activeTab === 'RSTB' || activeTab === 'YAML';
  const rootStyle = activeTab !== "COMPARER" ? {} : isComparerWorking ? {backgroundColor: "#2E303C"} : {};
  return (
    <div className="maincontainer" > 
      <MenuBarDisplay />
      <ActiveTabDisplay activeTab={activeTab} setActiveTab={setActiveTab} labelTextDisplay={labelTextDisplay} />
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
      <SearchTextInSarcPrompt
        setStatusText={setStatusText}
        setpaths={setpaths}
        searchInSarcQuery={searchInSarcQuery}
        setSearchInSarcQuery={setSearchInSarcQuery}
        isSearchInSarcOpened={isSearchInSarcOpened}
        setIsSearchInSarcOpened={setIsSearchInSarcOpened}>
      </SearchTextInSarcPrompt>

      <ButtonsDisplay
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
      />
      {<DirectoryTree
        onNodeSelect={handleNodeSelect}
        sarcPaths={paths}
        //For buttons clicks
        setStatusText={setStatusText}
        activeTab={activeTab}
        style={{ display: activeTab === 'SARC' ? "block" : "none" }}
      />}
      {<RstbTree
        onNodeSelect={handleNodeSelect}
        sarcPaths={paths}
        //For buttons clicks
        setStatusText={setStatusText}
        activeTab={activeTab}
        style={{ display: activeTab === 'RSTB' ? "block" : "none" }}
      />}
      {<Comparer setStatusText={setStatusText} activeTab={activeTab}/>}
      

      <div ref={editorContainerRef} className="code_editor" style={{ display: activeTab === 'YAML' ? "block" : "none" }}></div>
      {/* <div className="statusbar" style={statusStyle}>Current path: "{selectedPath.path} {selectedPath.endian}"</div> */}
      <div className="statusbar" style={statusStyle}>{statusText}</div>
    </div>
  );
}

export default App;
