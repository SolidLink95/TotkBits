
import { debounce } from "lodash"; // or any other method/utility to debounce
import * as monaco from "monaco-editor";
import React, { useEffect,useCallback  } from "react";
import ActiveTabDisplay from "./ActiveTab";
import AddOrRenameFilePrompt from './AddOrRenameFilePrompt'; // Import the modal component
import "./App.css";
import { SearchTextInSarcPrompt } from './SearchTextInSarc';
import { processArgv1 } from './ButtonClicks';
import ButtonsDisplay from "./Buttons";
import DirectoryTree from "./DirectoryTree";
import MenuBarDisplay from "./MenuBar";
import RstbTree from "./RstbTree";
import { useEditorContext } from './StateManager';
import { invoke } from '@tauri-apps/api/tauri';





function App() {

  const handleDrop = useCallback((event) => {
    console.log('Dropped:', event.dataTransfer.items);
    event.preventDefault();
    const files = event.dataTransfer.items;
    console.log(files);
    if (files.length > 0 && files[0].kind === 'file') {
      const file = files[0].getAsFile();
      if (file && !file.type.includes('directory')) {
        console.log('Dropped file:', file);
        // window.backend.openFile(file.path);
      }
    }
  }, []);
  const handleDragOver = useCallback((event) => {
    event.preventDefault();
    const files = event.dataTransfer.items;
    console.log(files);
    if (files.length > 0 && files[0].kind === 'file') {
      const file = files[0].getAsFile();
      if (file && !file.type.includes('directory')) {
        console.log('Dropped file:', file);
        // window.backend.openFile(file.path);
      }
    }
  }, []);

  const BackendEnum = {
    SARC: 'SARC',
    YAML: 'YAML',
    RSTB: 'RSTB',
  };

  const {
    searchInSarcQuery, setSearchInSarcQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal
  } = useEditorContext();



  const handleNodeSelect = (path, isfile) => {
    setSelectedPath({ path, isfile });
    console.log(`Selected Node Path in App: ${path} isfile: ${isfile}`);
    // Here you can use selectedPath for any other logic in App.jsx
  };


  useEffect(() => {
    // Initialize the Monaco editor only once
    if (!editorRef.current && editorContainerRef.current) {
      console.log("Initializing Monaco editor");
      editorRef.current = monaco.editor.create(editorContainerRef.current, {
        value: editorValue,
        language: lang,
        theme: "vs-dark",
        minimap: { enabled: false },
        wordWrap: 'on', // Enable word wrapping
      });
      console.log("Checking argv[1]");
      try {
        invoke('get_command_line_arg')
          .then((arg) => {
            if (arg && arg !== "" && arg !== null && arg !== undefined) {
              console.log('Received command-line argument:', arg);
              processArgv1(arg, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);

              // Perform actions based on the argument
            } else {
              console.log('No command-line argument provided.');
            }
          })
          .catch((error) => console.error('Error fetching command-line argument:', error));
      } catch (error) {
        console.error('Failed to fetch command-line argument:', error);
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
    color: statusText.toLowerCase().includes("error") ? 'red' :
      statusText.toLowerCase().includes('warning') ? 'yellow' : 'white',
  };



  return (
    <div onDrop={handleDrop} onDragOver={handleDragOver} className="maincontainer">
      <MenuBarDisplay />
      <ActiveTabDisplay activeTab={activeTab} setActiveTab={setActiveTab} labelTextDisplay={labelTextDisplay} />
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
      {/* <h2>Add File to SARC</h2> */}

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
      {/* {activeTab === 'SARC' && <DirectoryTree onNodeSelect={handleNodeSelect} sarcPaths={paths} />} */}
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
      <div ref={editorContainerRef} className="code_editor" style={{ display: activeTab === 'YAML' ? "block" : "none" }}></div>
      {/* <div className="statusbar" style={statusStyle}>Current path: "{selectedPath.path} {selectedPath.endian}"</div> */}
      <div className="statusbar" style={statusStyle}>{statusText}</div>
    </div>
  );
}

export default App;
