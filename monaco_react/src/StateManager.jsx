import React, { createContext, useContext, useState, useRef } from 'react';

const EditorContext = createContext();

export const EditorProvider = ({ children }) => {
    const [activeTab, setActiveTab] = useState('SARC'); // Adjust this initial value as needed
    const editorContainerRef = useRef(null); //monaco editor container
    const editorRef = useRef(null); //monaco editor reference
    const [editorValue, setEditorValue] = useState(''); //monaco editor content
    const [lang, setLang] = useState('yaml'); //monaco editor content
    const [statusText, setStatusText] = useState("safdsafd"); //status bar text
    const [selectedPath, setSelectedPath] = useState({ path: "", endian: "" }); //selected path from directory tree
    const [labelTextDisplay, setLabelTextDisplay] = useState({ sarc: '', yaml: '' }); //labeltext display near tabs
    const [paths, setpaths] = useState({ paths: [], added_paths: [], modded_paths: [] }); //paths structures for directory tree
    const [isModalOpen, setIsModalOpen] = useState(false);


    const updateEditorContent = (content) => {
        //setText(content);
        if (editorRef.current) {
          editorRef.current.setValue(content);
          //console.log(content);
        } 
      };
      const changeModal = () => setIsModalOpen(!isModalOpen);

    // Combine all states and functions into a single object
    const value = {
        activeTab, setActiveTab,
        editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
        statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
        paths, setpaths, isModalOpen, setIsModalOpen ,updateEditorContent, changeModal
    };

    return <EditorContext.Provider value={value}>{children}</EditorContext.Provider>;
};

// Custom hook for accessing the context
export const useEditorContext = () => useContext(EditorContext);
