import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import { clearSearchInSarcClick, restartApp, closeAllFilesClick,editConfigFileClick, editInternalSarcFile, extractFileClick, fetchAndSetEditorContent, saveAsFileClick, saveFileClick, useExitApp } from './ButtonClicks';

import { useEditorContext } from './StateManager';

function MenuBarDisplay() {
  // const [backupPaths, setBackupPaths] = useState({ paths: [], added_paths: [], modded_paths: [] }); //paths structures for directory tree

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

  const handleSearchClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    setIsSearchInSarcOpened(!isSearchInSarcOpened);
  }

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
      if (selectedPath.isfile === true) {
      extractFileClick(selectedPath, setStatusText);
      } else {
        setStatusText(`Select a file to extract, not directory ${selectedPath.path}`);
      }
    } else {
      setStatusText("Switch to SARC tab to extract files");
    }
  }

  const handleShowAllClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery);
    // if (backupPaths.paths.length > 0) {
    //   setpaths({ paths: backupPaths.paths, added_paths: backupPaths.added_paths, modded_paths: backupPaths.modded_paths });
    //   setBackupPaths({ paths: [], added_paths: [], modded_paths: [] });
    // } else {
    //   setBackupPaths(paths);
    // }
    // setStatusText(`Showing all sarc files` );
  }

  const handleShowAddedClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    // console.log(backupPaths.paths.length);
    // if (backupPaths.paths.length === 0) {
    //   setBackupPaths(paths);
    // }
    setpaths({ paths: paths.added_paths, added_paths: paths.added_paths, modded_paths: paths.modded_paths });
    setStatusText(`Showing only added files (${paths.added_paths.length})` );
  }

  const handleShowModdedClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    // if (backupPaths.paths.length === 0) {
    //   setBackupPaths(paths);
    // }
    setpaths({ paths: paths.modded_paths, added_paths: paths.added_paths, modded_paths: paths.modded_paths });
    setStatusText(`Showing only modded files (${paths.modded_paths.length})` );
  }


  const handleCloseAllFilesClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    closeAllFilesClick(setStatusText, setpaths, updateEditorContent, setLabelTextDisplay);
  }

  const editConfigFile = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    editConfigFileClick(setStatusText);
  }

  const restartAppClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    restartApp(setStatusText);
  }

  const handleClearSearchTextInSarc = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery);
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

  // useEffect(() => {
  //   // Reset backupPaths only if paths has been altered (not on initial render)
  //   if (Object.keys(paths).length > 0) {
  //     setBackupPaths({ paths: [], added_paths: [], modded_paths: [] });
  //   }
  // }, [labelTextDisplay]); //only when opened file is changed 

  return (
    <div className="menu-bar">
      <div className="menu-item" onClick={() => toggleDropdown('file')} ref={el => dropdownRefs.current.file = el}>
        File
        <div className="dropdown-content" style={{ display: showDropdown.file ? 'block' : 'none' }}>
          <a href="#" onClick={handleOpenFileClick}>Open</a>
          <a href="#" onClick={handleSaveClick}>Save</a>
          <a href="#" onClick={handleSaveAsClick}>Save as</a>
          <a href="#" onClick={handleCloseAllFilesClick}>Close all</a>
          <a href="#" onClick={editConfigFile}>Edit config</a>
          <a href="#" onClick={restartAppClick}>Restart</a>
          <a href="#" onClick={useExitApp}>Exit</a >
        </div>
      </div>
      {activeTab == "SARC" && <div className="menu-item" onClick={() => toggleDropdown('tools')} ref={el => dropdownRefs.current.tools = el}>
        Tools
        <div className="dropdown-content" style={{ display: showDropdown.tools ? 'block' : 'none' }}>
          <a href="#" onClick={handleSearchClick}>Search in sarc</a>
          {searchInSarcQuery.length > 0 && <a href="#" onClick={handleClearSearchTextInSarc}>Clear search</a>}
          <a href="#" onClick={handleAddClick}>Add file</a>
          <a href="#" onClick={handleOpenInternalSarcFile}>Edit</a>
          <a href="#" onClick={handleExtractClick}>Extract</a>
          {(paths.added_paths.length > 0 || paths.modded_paths.length > 0 ) && <a href="#" onClick={handleShowAllClick}>Show all</a>}
          {paths.added_paths.length > 0 && <a href="#" onClick={handleShowAddedClick}>Show added</a>}
          {paths.modded_paths.length > 0 && <a href="#" onClick={handleShowModdedClick}>Show modded</a>}
        </div>
      </div>}

    </div>
  );
}

export default MenuBarDisplay;
