import React, { createContext, useContext, useEffect, useRef, useState } from 'react';
import * as monaco from 'monaco-editor';

const EditorContext = createContext();



export const EditorProvider = ({ children }) => {
  const [activeTab, setActiveTab] = useState('SARC'); // Adjust this initial value as needed
  const [physicsMergeReturnTab, setPhysicsMergeReturnTab] = useState('SARC');
  const editorContainerRef = useRef(null); //monaco editor container
  const editorRef = useRef(null); //monaco editor reference
  const documentModels = useRef(new Map());
  const documentViewStates = useRef(new Map());
  const documentSnapshots = useRef(new Map());
  const rightEditorContainerRef = useRef(null);
  const rightEditorRef = useRef(null);
  const [rightDocumentId, setRightDocumentId] = useState(null);
  const [splitRatio, setSplitRatio] = useState(0.5);
  const [editorValue, setEditorValue] = useState(''); //monaco editor content
  const [readOnly, setReadOnly] = useState(false);
  useEffect(() => {
    editorRef.current?.updateOptions({ readOnly, domReadOnly: readOnly });
  }, [readOnly]);
  const [lang, setLang] = useState('yaml'); //monaco editor content
  const [statusText, setStatusText] = useState("Ready"); //status bar text
  const [renamePromptMessage, setRenamePromptMessage] = useState({ message: "Rename internal SARC file:", path: "" });
  const [selectedPath, setSelectedPath] = useState({ path: "", isfile: false }); //selected path from directory tree
  const [labelTextDisplay, setLabelTextDisplay] = useState({ sarc: '', yaml: '', rstb: '', comparer: '' }); //labeltext display near tabs
  const [paths, setpaths] = useState({ paths: [], added_paths: [], modded_paths: [], nested_paths: {}, file_type: '', root_name: '' }); //paths structures for directory tree
  const [pathsFilters, setPathsFilters] = useState({ showAll: true, showAdded: false, showModded: false });
  const [treeExpandedNodes, setTreeExpandedNodes] = useState(new Set());
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isAddPrompt, setIsAddPrompt] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [savingFile, setSavingFile] = useState('');
  // const [isUpdateNeeded, setIsUpdateNeeded] = useState(false);
  const [updateState, setUpdateState] = useState({wasChecked: false, isUpdateNeeded: false, latestVersion: ''});

  const [compareData, setCompareData] = useState({ decision: 'FilesFromDisk', 
                      content1: '', content2: '', filepath1: '', filepath2: '', 
                      isSmall: true, isFromDisk: false, isInternal: false,
                      label1: '', label2: '', isTiedToMonaco: false, lang: 'yaml'
                     }); //compare files content

  const [settings, setSettings] = useState({ argv1: '', argv: [],
    uiScale: 1,
    fontSize: 14, 
    theme: 'vs-dark', 
    minimap: false, 
    lang: "yaml", 
    contextMenuFontSize: 14 
  }); //settings for monaco editor

  const [isSearchInSarcOpened, setIsSearchInSarcOpened] = useState(false);
  const [searchInSarcQuery, setSearchInSarcQuery] = useState("");
  const [treeFilterQuery, setTreeFilterQuery] = useState("");

  const [config, setConfig] = useState({});
  const [aocModelCatalog, setAocModelCatalog] = useState(null);
  const [lm3SlotCatalog, setLm3SlotCatalog] = useState(null);
  const [modelBrowserSource, setModelBrowserSource] = useState('aoc');
  const [configLoading, setConfigLoading] = useState(false);
  const [isOptionsOpen, setIsOptionsOpen] = useState(false);

  const updateEditorContent = (content, lang, nextReadOnly = false) => {
    setReadOnly(Boolean(nextReadOnly));
    //setText(content);
    if (editorRef.current) {
      editorRef.current.setValue(content);
      if (lang) monaco.editor.setModelLanguage(editorRef.current.getModel(), lang);
    }
  };
  const changeModal = () => {
    setIsAddPrompt(true);
    setIsModalOpen(!isModalOpen)

  };

  // Combine all states and functions into a single object
  const value = {
    isOptionsOpen, setIsOptionsOpen,
    config, setConfig, aocModelCatalog, setAocModelCatalog,
    lm3SlotCatalog, setLm3SlotCatalog, modelBrowserSource, setModelBrowserSource,
    configLoading, setConfigLoading,
    updateState, setUpdateState,
    compareData, setCompareData,
    documentModels, documentViewStates, documentSnapshots,
    rightEditorContainerRef, rightEditorRef, rightDocumentId, setRightDocumentId,
    splitRatio, setSplitRatio,
    settings, setSettings,
    searchInSarcQuery, setSearchInSarcQuery, treeFilterQuery, setTreeFilterQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab, physicsMergeReturnTab, setPhysicsMergeReturnTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang, readOnly, setReadOnly,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, pathsFilters, setPathsFilters, treeExpandedNodes, setTreeExpandedNodes,
    isModalOpen, setIsModalOpen, updateEditorContent, changeModal,
    isLoading, setIsLoading,
    savingFile, setSavingFile
  };

  return <EditorContext.Provider value={value}>{children}</EditorContext.Provider>;
};

// Custom hook for accessing the context
export const useEditorContext = () => useContext(EditorContext);
