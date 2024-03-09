
import { listen } from '@tauri-apps/api/event';
import { invoke } from "@tauri-apps/api/tauri";
import { debounce } from "lodash"; // or any other method/utility to debounce
import * as monaco from "monaco-editor";
import React, { useEffect, useRef, useState } from "react";
import ActiveTabDisplay from "./ActiveTab";
import "./App.css";
import ButtonsDisplay from "./Buttons";
import MenuBarDisplay from "./MenuBar";
import DirectoryTree from "./PathList";



function App() {
  const BackendEnum = {
    SARC: 'SARC',
    YAML: 'YAML',
    RSTB: 'RSTB',
  };
  const [activeTab, setActiveTab] = useState('SARC'); // Adjust this initial value as needed
  const editorContainerRef = useRef(null);
  const activetabRef = useRef(null);
  const editorRef = useRef(null);
  const [statusText, setStatusText] = useState("Ready");
  const [selectedPath, setSelectedPath] = useState({ path: "", endian: "" });
  const [labelTextDisplay, setLabelTextDisplay] = useState({ sarc: '', yaml: '' });
  const [paths, setpaths] = useState({paths: [], added_paths: [], modded_paths: []});
  const [editorValue, setEditorValue] = useState('');
  const [openedData, setOpenedData] = useState(null);


  // const sarcPaths = {
  //   "paths": [
  //     "folder1/subfolder1/file1.txt",
  //     "folder1/subfolder1/file11.txt",
  //     "folder1/subfolder2/file2.txt",
  //     "folder2/subfolder1/file3.txt",
  //     "folder3/file4.txt",
  //   ],
  //   "added_paths": [
  //     "folder1/subfolder1/file11.txt",
  //   ],
  //   "modded_paths": [
  //     "folder3/file4.txt",
  //   ]
  // };

  useEffect(() => {
    const unlisten = listen('opened_data_from_backend', (event) => {
      console.log('Received user info from backend:', event.payload);
      setOpenedData(event.payload);
      updateEditorContent(openedData.text);
      setStatusText(openedData.status_text);
    });

    return () => {
      unlisten.then((unlistenFn) => unlistenFn());
    };
  }, []);


  const fetchStatusString = async () => {
    try {
      const statusText = await invoke('get_status_text'); // Match the command name
      setStatusText(statusText ||"Ready XXX"); // Set the status text (or handle it as needed
      console.log(statusText);
    } catch (e) {
      console.error('Failed to get status text', e);
    }
  }

  const updateEditorContent = (content) => {
    //setText(content);
    if (editorRef.current) {
      editorRef.current.setValue(content);
      console.log(content);
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
        language: "yaml",
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

  const sendTextToRust = async () => {
    const editorText = editorRef.current.getValue(); // Get current text from Monaco Editor
    try {
      await invoke('receive_text_from_editor', { text: editorText }); // Invoke the Rust command
      console.log('Text sent to Rust backend successfully.');
    } catch (error) {
      console.error('Failed to send text to Rust backend:', error);
    }
  };
  const statusStyle = {
    color: statusText.toLowerCase().includes("error") ? 'red' : 
      statusText.toLowerCase().includes('warning') ? 'yellow' : 'white',
  };



  return (
    <div>
      <MenuBarDisplay />
      <ActiveTabDisplay activeTab={activeTab} setActiveTab={setActiveTab} labelTextDisplay={labelTextDisplay} />
      <ButtonsDisplay updateEditorContent={updateEditorContent} setStatusText={setStatusText} setActiveTab={setActiveTab} setLabelTextDisplay={setLabelTextDisplay} setpaths={setpaths} />
      {activeTab === 'SARC' && <DirectoryTree onNodeSelect={handleNodeSelect} sarcPaths={paths} />}
      <div ref={editorContainerRef} className="code_editor" style={{  display: activeTab === 'YAML' ? "block" : "none" }}></div>
      {/* <div className="statusbar" style={statusStyle}>Current path: "{selectedPath.path} {selectedPath.endian}"</div> */}
      <div className="statusbar" style={statusStyle}>{statusText}</div>
    </div>
  );
}

export default App;
