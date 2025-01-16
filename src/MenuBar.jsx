import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method
import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import { ImageButton } from "./Buttons";
import { clearSearchInSarcClick, closeAllFilesClick, editConfigFileClick, editInternalSarcFile, extractFileClick, fetchAndSetEditorContent, restartApp, saveAsFileClick, saveFileClick, useExitApp } from './ButtonClicks';
import { clearCompareData, compareFilesByDecision, compareInternalFileWithOVanila, compareInternalFileWithOVanilaMonaco } from './Comparer';
import { useEditorContext } from './StateManager';
import { set } from 'lodash';

function MenuBarDisplay() {
  // const [backupPaths, setBackupPaths] = useState({ paths: [], added_paths: [], modded_paths: [] }); //paths structures for directory tree

  const {
    searchInSarcQuery, setSearchInSarcQuery, isUpdateNeeded, setIsUpdateNeeded,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal,
    compareData, setCompareData,
  } = useEditorContext();

  const [showDropdown, setShowDropdown] = useState({ file: false, view: false, tools: false, compare: false });
  const dropdownRefs = useRef({ file: null, view: null, tools: null, compare: null });

  const closeMenu = () => {
    setShowDropdown({ file: false, view: false, tools: false, compare: false });
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

  const handleCompareFileInternalWithVanila = async (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    try {
      if (activeTab === 'SARC') {
        compareInternalFileWithOVanila(selectedPath.path, setStatusText, setActiveTab, setCompareData);
      } else if (activeTab === 'YAML') {
        //empty internal path, irrelevant
        compareInternalFileWithOVanilaMonaco(setStatusText, setActiveTab, setCompareData, editorRef);
      } else {
        setStatusText("Switch to SARC or YAML tab to compare files!"); //should be unreachable
        return;
      }
      if (activeTab === 'COMPARER') {
        console.log('Menubarjsx: Files compared successfully');
      }
    } catch (error) {
      console.error('Failed to compare files: ', error);
    }
  };

  const handleClearCompareData = async (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    clearCompareData(setCompareData);
    setStatusText("Compare data cleared");
  };

  const handleCompareFilesDisk = async (event, isFromDisk) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    try {
      setCompareData((prevData) => ({
        ...prevData,
        decision: 'FilesFromDisk', // simplest decision, no other arguments needed
      }));
      // compareFilesByDecision('', setStatusText, activeTab, setActiveTab, compareData, setCompareData, editorRef, 'FilesFromDisk', isFromDisk);
      compareFilesByDecision(setStatusText, setActiveTab, setCompareData, editorRef, isFromDisk, setLabelTextDisplay);

      const success = activeTab === 'COMPARER';
      if (success) {
        console.log('Menubarjsx: Files compared successfully');
      }
    } catch (error) {
      console.error('Failed to compare files: ', error);
    }
  };

  //Poorly, but works
  const handleCompareFilesFromDisk = (event) => handleCompareFilesDisk(event, true);
  const handleCompareMonacoEditorFromDisk = (event) => handleCompareFilesDisk(event, false);
  // const handleCompareInternalFromDisk= (event) => handleCompareFileInternal(event, "InternalFileWithFileFromDisk", true);
  // const handleCompareMonacoInternalFromDisk= (event) => handleCompareFileInternal(event, "InternalFileWithFileFromDisk", false);

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
    closeAllFilesClick(setCompareData, setStatusText, setpaths, updateEditorContent, setLabelTextDisplay);
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
      ...{ file: false, view: false, tools: false, compare: false }, // Reset all to false
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
  const blankIcon = 'menu/blank.png';

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
    { label: 'Extract sarc contents', onClick: handleExtractOpenedSarc, icon: 'context_menu/extract_all.png', shortcut: '', condition: true },
    { label: 'Search in sarc', onClick: handleSearchClick, icon: 'menu/lupa.png', shortcut: '', condition: true },
    { label: 'Clear search', onClick: handleClearSearchTextInSarc, icon: 'menu/clear_search.png', shortcut: '', condition: searchInSarcQuery.length > 0 },
    { label: 'Add file', onClick: handleAddClick, icon: 'menu/add.png', shortcut: '', condition: true },
    { label: 'Edit', onClick: handleOpenInternalSarcFile, icon: 'context_menu/edit.png', shortcut: '', condition: true },
    { label: 'Extract file', onClick: handleExtractClick, icon: 'context_menu/extract.png', shortcut: '', condition: paths.paths.length > 0 && selectedPath.isfile },
    { label: 'Show all', onClick: handleShowAllClick, icon: blankIcon, shortcut: '', condition: paths.added_paths.length > 0 || paths.modded_paths.length > 0 },
    { label: 'Show added', onClick: handleShowAddedClick, icon: blankIcon, shortcut: '', condition: paths.added_paths.length > 0 },
    { label: 'Show modded', onClick: handleShowModdedClick, icon: blankIcon, shortcut: '', condition: paths.modded_paths.length > 0 }
  ];
  const isSelectedPathInPaths = () => {
    for (const path of paths.paths) {
      if (path === selectedPath.path) {
        console.log(path);
        return true;
      }
    }
    return false;
  }
  const compToVanLabel = activeTab === "SARC" ? "Selected to vanila" : "This to vanila";
  // const selToVanCond = (activeTab === "SARC") || (activeTab === "YAML") && paths.paths.some(path => path === selectedPath.path);
  const selToVanCond = (activeTab === "SARC" && paths.paths.some(path => path === selectedPath.path)) || (activeTab === "YAML" && labelTextDisplay.yaml?.length > 0);
  const compareMenuItems = [
    { label: 'Files', onClick: handleCompareFilesFromDisk, icon: blankIcon, shortcut: '', condition: true },
    { label: 'This to file', onClick: handleCompareMonacoEditorFromDisk, icon: blankIcon, shortcut: '', condition: activeTab === "YAML" && labelTextDisplay.yaml?.length > 0 },
    { label: compToVanLabel, onClick: handleCompareFileInternalWithVanila, icon: blankIcon, shortcut: '', condition: selToVanCond },
    { label: 'Clear', onClick: handleClearCompareData, icon: blankIcon, shortcut: '', condition: activeTab === "COMPARER" && compareData.content1 !== '' },

  ];
  const menuSpanStyle = { marginLeft: '20px', color: '#bcbcbc' };
  const menuDivStyle = { display: 'flex', alignItems: 'center' };
  const menuItemStyle = { display: 'flex', alignItems: 'center' };
  const menuItemImgStyle = { marginRight: '10px', width: iconSize, height: iconSize };

  return (
    <div>
      <div className="menu-bar" >

        <div className="menu-item" onClick={() => toggleDropdown('file')} ref={el => dropdownRefs.current.file = el}>
          File
          <div className="dropdown-content" style={{ display: showDropdown.file ? 'block' : 'none' }}>
            {fileMenuItems.map((item, id) => (
              <li
                key={id}
                className="menu-item"
                onClick={item.onClick}
                style={menuItemStyle}
              >
                <div style={menuDivStyle}>
                  <img src={item.icon} alt={item.label} style={menuItemImgStyle} />
                  {item.label}
                </div>
                <span style={menuSpanStyle}>{item.shortcut}</span>
              </li>
            ))}
          </div>
        </div>
        <div className="menu-item" onClick={() => toggleDropdown('compare')} ref={el => dropdownRefs.current.compare = el}>
          Compare
          <div className="dropdown-content" style={{ display: showDropdown.compare ? 'block' : 'none' }}>
            {compareMenuItems.map((item, id) => (
              item.condition ? (<li
                key={id}
                className="menu-item"
                onClick={item.onClick}
                style={menuItemStyle}
              >
                <div style={menuDivStyle}>
                  <img src={item.icon} alt={item.label} style={menuItemImgStyle} />
                  {item.label}
                </div>
                <span style={menuSpanStyle}>{item.shortcut}</span>
              </li>
              ) : null))}
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
                  style={menuItemStyle}
                >
                  <div style={menuDivStyle}>
                    <img src={item.icon} alt={item.label} style={menuItemImgStyle} />
                    {item.label}
                  </div>
                  <span style={menuSpanStyle}>{item.shortcut}</span>
                </li>
                ) : null))}
            </div>
          </div>
        )}
      </div>
    </div>
  );


}

function MenuBarDisplayWithUpdater() {
  const {
    updateState, setUpdateState, setStatusText
  } = useEditorContext();
  const handleUpdateClick = async (event) => {
    console.log("Update button clicked!");

    try {
      if (updateState.latestVersion === '') {
        setStatusText('ERROR: No update available');
        return;
      }
      const content = await invoke('update_app', { latestVer: updateState.latestVersion });
      console.log(content);
      const msg = content ?? '';
      if (msg !== '') {
        setStatusText(msg);
      }
    } catch (error) {
      console.error('Failed to update app: ', error);
    }
  }
  const iconSize = '28px';
  const isUp = updateState.isUpdateNeeded;
  return (
    <div style={{
      display: 'flex',
      justifyContent: 'space-between',
      backgroundColor: '#333',
      // fontWeight: 'bold',
    }}>
      <MenuBarDisplay />
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <ImageButton
          key={isUp ? 'UpdaterButton' : 'NoUpdaterButton'}
          src={isUp ? 'update.png' : 'noupdate.png'}
          alt={
            isUp
              ? `Update to ${updateState.latestVersion}`
              : 'Totkbits is up to date'
          }
          onClick={isUp ? handleUpdateClick : null}
          title={
            isUp
              ? `Update to ${updateState.latestVersion}`
              : 'Totkbits is up to date'
          }
          style={{
            padding: '5px',
            backgroundColor: '#232529',
            width: iconSize,
            height: iconSize,
          }}
        />
      </div>
    </div>
  );
}

export { MenuBarDisplay, MenuBarDisplayWithUpdater };
// export default MenuBarDisplayWithUpdater;
