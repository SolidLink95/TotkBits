import React, { createContext, useContext, useRef, useState } from 'react';
import * as monaco from "monaco-editor";

const EditorContext = createContext();

export const EditorProvider = ({ children }) => {
  const [activeTab, setActiveTab] = useState('SARC'); // Adjust this initial value as needed
  const editorContainerRef = useRef(null); //monaco editor container
  const editorRef = useRef(null); //monaco editor reference
  const [editorValue, setEditorValue] = useState(''); //monaco editor content
  const [lang, setLang] = useState('yaml'); //monaco editor content
  const [statusText, setStatusText] = useState("Ready"); //status bar text
  const [renamePromptMessage, setRenamePromptMessage] = useState({ message: "Rename internal SARC file:", path: "" }); //status bar text
  const [selectedPath, setSelectedPath] = useState({ path: "", isfile: false }); //selected path from directory tree
  const [labelTextDisplay, setLabelTextDisplay] = useState({ sarc: '', yaml: '',rstb: '' }); //labeltext display near tabs
  const [paths, setpaths] = useState({ paths: [], added_paths: [], modded_paths: [] }); //paths structures for directory tree
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isAddPrompt, setIsAddPrompt] = useState(false);


  const updateEditorContent = (content, lang) => {
    //setText(content);
    if (editorRef.current) {
      editorRef.current.setValue(content);
      //Doesnt work without custom web workers
      // const model = editorRef.current.getModel();
  
      // monaco.editor.setModelLanguage(model, lang);
      //console.log(content);
    }
  };
  const changeModal = () => { 
    setIsAddPrompt(true);
    setIsModalOpen(!isModalOpen) 
  
  };

  // Combine all states and functions into a single object
  const value = {
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal
  };

  return <EditorContext.Provider value={value}>{children}</EditorContext.Provider>;
};

// Custom hook for accessing the context
export const useEditorContext = () => useContext(EditorContext);
