import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import { extractFileClick, closeAllFilesClick, editInternalSarcFile, fetchAndSetEditorContent, saveAsFileClick, saveFileClick, useExitApp } from './ButtonClicks';

import { useEditorContext } from './StateManager';

function MenuBarDisplay() {

  const {
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal
  } = useEditorContext();

  const [showDropdown, setShowDropdown] = useState({ file: false, view: false, tools: false });
  const dropdownRefs = useRef({ file: null, view: null, tools: null });

  const closeMenu = () => {
    setShowDropdown({ file: false, view: false, tools: false });
  };


  //Buttons functions
  const handleOpenFileClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
  };
  const handleOpenInternalSarcFile = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    if (activeTab === 'SARC') {
      editInternalSarcFile(selectedPath.path, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent);
    } else {
      setStatusText("Switch to SARC tab to edit files");
    }
  };
  const handleSaveClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    saveFileClick(setStatusText, activeTab, setpaths, editorRef);
  };
  const handleSaveAsClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    saveAsFileClick(setStatusText, activeTab, setpaths, editorRef);
  };
  const handleAddClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    if (activeTab === 'SARC') {
      setIsAddPrompt(true);
      setIsModalOpen(true);
    } else {
      setStatusText("Switch to SARC tab to add files");
    }
  }
  const handleExtractClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent () => extractFileClick(selectedPath, setStatusText)
    closeMenu();
    if (activeTab === 'SARC') {
      extractFileClick(selectedPath, setStatusText);
    } else {
      setStatusText("Switch to SARC tab to extract files");
    }
  }


  const handleCloseAllFilesClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    closeAllFilesClick(setStatusText, setpaths, updateEditorContent, setLabelTextDisplay);
  }

  const toggleDropdown = (menu) => {
    setShowDropdown(prevState => ({
      ...{ file: false, view: false, tools: false }, // Reset all to false
      [menu]: !prevState[menu] // Then toggle the clicked one
    }));
  };


  useEffect(() => {
    function handleClickOutside(event) {
      // Get an array of all dropdown DOM nodes
      const dropdownNodes = Object.values(dropdownRefs.current).filter(Boolean);
      // Check if the click target is not contained within any dropdown node
      const isOutside = dropdownNodes.every(node => !node.contains(event.target));
      if (isOutside) {
        closeMenu();
      }
    }

    // Add click event listener
    document.addEventListener('mousedown', handleClickOutside);

    // Cleanup event listener
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, []);

  return (
    <div className="menu-bar">
      <div className="menu-item" onClick={() => toggleDropdown('file')} ref={el => dropdownRefs.current.file = el}>
        File
        <div className="dropdown-content" style={{ display: showDropdown.file ? 'block' : 'none' }}>
          <a href="#" onClick={handleOpenFileClick}>Open</a>
          <a href="#" onClick={handleSaveClick}>Save</a>
          <a href="#" onClick={handleSaveAsClick}>Save as</a>
          <a href="#" onClick={handleCloseAllFilesClick}>Close all</a>
          <a href="#" onClick={useExitApp}>Exit</a >
        </div>
      </div>
      <div className="menu-item" onClick={() => toggleDropdown('tools')} ref={el => dropdownRefs.current.tools = el}>
        Tools
        <div className="dropdown-content" style={{ display: showDropdown.tools ? 'block' : 'none' }}>
          <a href="#" onClick={handleOpenInternalSarcFile}>Edit</a>
          <a href="#" onClick={handleExtractClick}>Extract</a>
          {/* <a href="#">Find</a> */}
          {/* <a href="#">Settings</a> */}
        </div>
      </div>
    </div>
  );
}

export default MenuBarDisplay;
