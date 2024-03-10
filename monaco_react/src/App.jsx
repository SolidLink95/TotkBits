
import { debounce } from "lodash"; // or any other method/utility to debounce
import * as monaco from "monaco-editor";
import React, { useEffect, useRef, useState } from "react";
import ActiveTabDisplay from "./ActiveTab";
import AddFilePrompt from './AddFilePrompt'; // Import the modal component
import "./App.css";
import ButtonsDisplay from "./Buttons";
import DirectoryTree from "./DirectoryTree";
import MenuBarDisplay from "./MenuBar";



function App() {
  const BackendEnum = {
    SARC: 'SARC',
    YAML: 'YAML',
    RSTB: 'RSTB',
  };
  const [activeTab, setActiveTab] = useState('SARC'); // Adjust this initial value as needed
  const editorContainerRef = useRef(null); //monaco editor container
  const editorRef = useRef(null); //monaco editor reference
  const [editorValue, setEditorValue] = useState(''); //monaco editor content
  const [lang, setLang] = useState('yaml'); //monaco editor content
  const [statusText, setStatusText] = useState("Ready"); //status bar text
  const [selectedPath, setSelectedPath] = useState({ path: "", endian: "" }); //selected path from directory tree
  const [labelTextDisplay, setLabelTextDisplay] = useState({ sarc: '', yaml: '' }); //labeltext display near tabs
  const [paths, setpaths] = useState({paths: [], added_paths: [], modded_paths: []}); //paths structures for directory tree
  const [openedData, setOpenedData] = useState(null);
  const [isModalOpen, setIsModalOpen] = useState(false);

  const changeModal = () => setIsModalOpen(!isModalOpen);


  //Functions

  const updateEditorContent = (content) => {
    //setText(content);
    if (editorRef.current) {
      editorRef.current.setValue(content);
      //console.log(content);
    } 
  };


  const handleNodeSelect = (path, endian) => {
    setSelectedPath({ path, endian });
    console.log(`Selected Node Path in App: ${path} endian: ${endian}`);
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
        wordWrap: 'on', // Enable word wrapping
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
    <div>
      <MenuBarDisplay />
      <ActiveTabDisplay activeTab={activeTab} setActiveTab={setActiveTab} labelTextDisplay={labelTextDisplay} />
      <AddFilePrompt isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} setStatusText={setStatusText} setpaths={setpaths}>
        <h2>Add File to SARC</h2>
      </AddFilePrompt>
      
      <ButtonsDisplay 
        editorRef={editorRef} 
        updateEditorContent={updateEditorContent} 
        setStatusText={setStatusText} 
        activeTab={activeTab} 
        setActiveTab={setActiveTab} 
        setLabelTextDisplay={setLabelTextDisplay} 
        setpaths={setpaths} 
        selectedPath={selectedPath} 
        changeModal={changeModal} 
      />
      {/* {activeTab === 'SARC' && <DirectoryTree onNodeSelect={handleNodeSelect} sarcPaths={paths} />} */}
      {<DirectoryTree 
        onNodeSelect={handleNodeSelect} 
        sarcPaths={paths} 
        style={{  display: activeTab === 'SARC' ? "block" : "none" }}
      />}
      <div ref={editorContainerRef} className="code_editor" style={{  display: activeTab === 'YAML' ? "block" : "none" }}></div>
      {/* <div className="statusbar" style={statusStyle}>Current path: "{selectedPath.path} {selectedPath.endian}"</div> */}
      <div className="statusbar" style={statusStyle}>{statusText}</div>
    </div>
  );
}

export default App;
