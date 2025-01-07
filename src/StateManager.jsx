import React, { createContext, useContext, useRef, useState } from 'react';
import { DiffEditor } from '@monaco-editor/react';

const EditorContext = createContext();



export const EditorProvider = ({ children }) => {
  const [activeTab, setActiveTab] = useState('SARC'); // Adjust this initial value as needed
  const editorContainerRef = useRef(null); //monaco editor container
  const editorRef = useRef(null); //monaco editor reference
  const [editorValue, setEditorValue] = useState(''); //monaco editor content
  const [lang, setLang] = useState('yaml'); //monaco editor content
  const [statusText, setStatusText] = useState("Ready"); //status bar text
  const [renamePromptMessage, setRenamePromptMessage] = useState({ message: "Rename internal SARC file:", path: "" });
  const [selectedPath, setSelectedPath] = useState({ path: "", isfile: false }); //selected path from directory tree
  const [labelTextDisplay, setLabelTextDisplay] = useState({ sarc: '', yaml: '', rstb: '', comparer: '' }); //labeltext display near tabs
  const [paths, setpaths] = useState({ paths: [], added_paths: [], modded_paths: [] }); //paths structures for directory tree
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isAddPrompt, setIsAddPrompt] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [isUpdateNeeded, setIsUpdateNeeded] = useState(false);

  const [compareData, setCompareData] = useState({ decision: 'FilesFromDisk', 
                      content1: '', content2: '', filepath1: '', filepath2: '', 
                      isSmall: true, isFromDisk: false, isInternal: false,
                      label1: '', label2: '', isTiedToMonaco: false, lang: 'yaml'
                     }); //compare files content

  const [settings, setSettings] = useState({ argv1: '', 
    fontSize: 14, 
    theme: 'vs-dark', 
    minimap: false, 
    lang: "yaml", 
    contextMenuFontSize: 14 
  }); //settings for monaco editor

  const [isSearchInSarcOpened, setIsSearchInSarcOpened] = useState(false);
  const [searchInSarcQuery, setSearchInSarcQuery] = useState("");


  const updateEditorContent = (content, lang) => {
    //setText(content);
    if (editorRef.current) {
      editorRef.current.setValue(content);
    }
  };
  const changeModal = () => {
    setIsAddPrompt(true);
    setIsModalOpen(!isModalOpen)

  };

  // Combine all states and functions into a single object
  const value = {
    isUpdateNeeded, setIsUpdateNeeded,
    compareData, setCompareData,
    settings, setSettings,
    searchInSarcQuery, setSearchInSarcQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal,
    isLoading, setIsLoading
  };

  return <EditorContext.Provider value={value}>{children}</EditorContext.Provider>;
};

// Custom hook for accessing the context
export const useEditorContext = () => useContext(EditorContext);
