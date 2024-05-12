import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import { clearSearchInSarcClick, restartApp, closeAllFilesClick, editConfigFileClick, editInternalSarcFile, extractFileClick, fetchAndSetEditorContent, saveAsFileClick, saveFileClick, useExitApp } from './ButtonClicks';
import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
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

  const handleExtractOpenedSarc = async (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    try {
      const content = await invoke('extract_opened_sarc');
      console.log(content);
      if (content !== null && content.status_text !== undefined) {
        setStatusText(content.status_text);
      }
    } catch (error) {
      console.error('Failed to extract sarc: ', error);
    }
  }

  const handleShowAllClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery);
  }

  const handleShowAddedClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    // console.log(backupPaths.paths.length);
    // if (backupPaths.paths.length === 0) {
    //   setBackupPaths(paths);
    // }
    setpaths({ paths: paths.added_paths, added_paths: paths.added_paths, modded_paths: paths.modded_paths });
    setStatusText(`Showing only added files (${paths.added_paths.length})`);
  }

  const handleShowModdedClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    // if (backupPaths.paths.length === 0) {
    //   setBackupPaths(paths);
    // }
    setpaths({ paths: paths.modded_paths, added_paths: paths.added_paths, modded_paths: paths.modded_paths });
    setStatusText(`Showing only modded files (${paths.modded_paths.length})`);
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
  const iconSize = '20px';

  const fileMenuItems = [
    { label: 'Open', onClick: handleOpenFileClick, icon: 'menu/open.png', shortcut: 'Ctrl+O' },
    { label: 'Save', onClick: handleSaveClick, icon: 'menu/save.png', shortcut: 'Ctrl+S' },
    { label: 'Save as', onClick: handleSaveAsClick, icon: 'menu/save_as.png', shortcut: 'Ctrl+Shift+S' },
    { label: 'Close all', onClick: handleCloseAllFilesClick, icon: 'menu/closeall.png', shortcut: '' },
    { label: 'Edit config', onClick: editConfigFile, icon: 'menu/edit_config.png', shortcut: '' },
    { label: 'Restart', onClick: restartAppClick, icon: 'menu/restart.png', shortcut: '' },
    { label: 'Exit', onClick: useExitApp, icon: 'menu/exit.png', shortcut: '' }
  ];

  const toolsMenuItems = [
    { label: 'Extract sarc contents', onClick: handleExtractOpenedSarc, icon: 'context_menu/extract.png', shortcut: '', condition: true },
    { label: 'Search in sarc', onClick: handleSearchClick, icon: 'menu/lupa.png', shortcut: '', condition: true },
    { label: 'Clear search', onClick: handleClearSearchTextInSarc, icon: 'menu/clear_search.png', shortcut: '', condition: searchInSarcQuery.length > 0 },
    { label: 'Add file', onClick: handleAddClick, icon: 'menu/add.png', shortcut: '', condition: true },
    { label: 'Edit', onClick: handleOpenInternalSarcFile, icon: 'context_menu/edit.png', shortcut: '', condition: true },
    { label: 'Extract file', onClick: handleExtractClick, icon: 'context_menu/extract.png', shortcut: '', condition: paths.paths.length > 0 && selectedPath.isfile },
    { label: 'Show all', onClick: handleShowAllClick, icon: 'menu/blank.png', shortcut: '', condition: paths.added_paths.length > 0 || paths.modded_paths.length > 0 },
    { label: 'Show added', onClick: handleShowAddedClick, icon: 'menu/blank.png', shortcut: '', condition: paths.added_paths.length > 0 },
    { label: 'Show modded', onClick: handleShowModdedClick, icon: 'menu/blank.png', shortcut: '', condition: paths.modded_paths.length > 0 }
  ];


  return (
    <div className="menu-bar">
      <div className="menu-item" onClick={() => toggleDropdown('file')} ref={el => dropdownRefs.current.file = el}>
        File
        <div className="dropdown-content" style={{ display: showDropdown.file ? 'block' : 'none' }}>
          {fileMenuItems.map((item, id) => (
            <li
              key={id}
              className="menu-item"
              onClick={item.onClick}
              style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}
            >
              <div style={{ display: 'flex', alignItems: 'center' }}>
                <img src={item.icon} alt={item.label} style={{ marginRight: '10px', width: iconSize, height: iconSize }} />
                {item.label}
              </div>
              <span style={{ marginLeft: '20px', color: '#bcbcbc' }}>{item.shortcut}</span>
            </li>
            ))}
        </div>
      </div>
      {activeTab === "SARC" && (
        <div className="menu-item" onClick={() => toggleDropdown('tools')} ref={el => dropdownRefs.current.tools = el}>
          Tools
          <div className="dropdown-content" style={{ display: showDropdown.tools ? 'block' : 'none' }}>
            {toolsMenuItems.map((item, id) => (
              item.condition ? (<li
                key={id}
                className="menu-item"
                onClick={item.onClick}
                style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}
              >
                <div style={{ display: 'flex', alignItems: 'center' }}>
                  <img src={item.icon} alt={item.label} style={{ marginRight: '10px', width: iconSize, height: iconSize }} />
                  {item.label}
                </div>
                <span style={{ marginLeft: '20px', color: '#bcbcbc' }}>{item.shortcut}</span>
              </li>
              ) : null))}
          </div>
        </div>
      )}
    </div>
  );


}

export default MenuBarDisplay;
