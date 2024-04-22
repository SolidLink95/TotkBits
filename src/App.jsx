
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/tauri';
import { debounce } from "lodash"; // or any other method/utility to debounce
import * as monaco from "monaco-editor";
import React, { useEffect, useRef, useState } from "react";
import ActiveTabDisplay from "./ActiveTab";
import AddOrRenameFilePrompt from './AddOrRenameFilePrompt'; // Import the modal component
import "./App.css";
import { OpenFileFromPath } from './ButtonClicks';
import ButtonsDisplay from "./Buttons";
import DirectoryTree from "./DirectoryTree";
import MenuBarDisplay from "./MenuBar";
import RstbTree from "./RstbTree";
import { SearchTextInSarcPrompt } from './SearchTextInSarc';
import { useEditorContext } from './StateManager';



let triggered = false

function App() {

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
  const canProcessEvent = useRef(true);
  useEffect(() => {
    const handleFileDrop = async ({ payload }) => {
      if (canProcessEvent.current && payload.length > 0) {
        canProcessEvent.current = false; // Set the flag to false to block processing
        const file = payload[0];
        // console.log(performance.now(), 'File dropped:', file);

        try {
          OpenFileFromPath(file, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
        } catch (error) {
          console.error('Error processing file:', error);
        }

        // Reset the flag after 0.7 seconds
        setTimeout(() => {
          canProcessEvent.current = true;
        }, 700);
      }
    };
    let unlisten = null; // Declare unlisten outside to ensure scope availability

    const setupListener = async () => {
      try {
        const listener = await listen('tauri://file-drop', handleFileDrop);
        unlisten = listener.unlisten; // Correctly assign the unlisten method
      } catch (error) {
        console.error('Failed to set up listener:', error);
      }
    };

    setupListener();

    // Cleanup function to remove the event listener
    return () => {
      if (unlisten) { // Check if unlisten is a function
        unlisten().catch(error => console.error('Error unlistening:', error));
      }
    };
  }, []
  );



  const handleNodeSelect = (path, isfile) => {
    setSelectedPath({ path, isfile });
    console.log(`Selected Node Path in App: ${path} isfile: ${isfile}`);
    // Here you can use selectedPath for any other logic in App.jsx
  };


  useEffect(() => {
    let startupData = { argv1: '', fontSize: 14 };
    // Initialize the Monaco editor only once
    if (!editorRef.current && editorContainerRef.current) {
      console.log("Initializing Monaco editor");
      console.log(startupData);
      editorRef.current = monaco.editor.create(editorContainerRef.current, {
        value: editorValue,
        language: lang,
        theme: "vs-dark",
        minimap: { enabled: false },
        wordWrap: 'on', // Enable word wrapping
        fontSize: startupData.fontSize,
      });
      try {
        invoke('get_startup_data')
          .then((data) => {
            const arg = data["argv1"] || "";
            const fontSize = data["fontSize"] || 14;
            editorRef.current.updateOptions({ fontSize: fontSize });
            console.log("Startup data:", data, arg, fontSize );
            startupData = { argv1: arg, fontSize: fontSize };
            console.log("Startup data set:", startupData);
            
            if (arg && arg !== "" && arg !== null && arg !== undefined) {
              console.log('Received command-line argument:', arg);
              OpenFileFromPath(arg, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
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
    <div className="maincontainer">
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
