import { open } from '@tauri-apps/plugin-shell';
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import "./App.css";
import { addFilesFromDirRecursivelyToRoot, clearSearchInSarcClick, closeAllFilesClick, editConfigFileClick, editInternalSarcFile, extractFileClick, extractRootFolderClick, fetchAndSetEditorContent, OpenFileFromPath, openFolderContent, restartApp, saveAsFileClick, saveFileClick, useExitApp } from './ButtonClicks';
import { ImageButton } from "./Buttons";
import CommandsHelp from './CommandsHelp';
import { clearCompareData, compareFilesByDecision, compareInternalFileWithOVanila, compareInternalFileWithOVanilaMonaco } from './Comparer';
import { getDocumentsSnapshot, openUtilityDocument, subscribeDocuments } from './DocumentState';
import { useEditorContext } from './StateManager';
import { invoke } from '@tauri-apps/api/core';
import { isFileTypeSaveable } from './FileTypes';
import { downloadMiiGlb } from './Mii';

function MenuBarDisplay({ updateButton = null }) {
  const { documents, activeDocumentId } = useSyncExternalStore(subscribeDocuments, getDocumentsSnapshot);
  const activeDocument = documents.find((document) => document.id === activeDocumentId);
  const fileMetadata = activeDocument?.fileMetadata || '';
  // const [backupPaths, setBackupPaths] = useState({ paths: [], added_paths: [], modded_paths: [] }); //paths structures for directory tree

  const {
    setIsOptionsOpen, isOptionsOpen,
    searchInSarcQuery, setSearchInSarcQuery, isUpdateNeeded, setIsUpdateNeeded,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    setPhysicsMergeReturnTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang, readOnly,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal,
    compareData, setCompareData,
    setSavingFile, documentSnapshots, aocModelCatalog, setAocModelCatalog,
  } = useEditorContext();

  const [showDropdown, setShowDropdown] = useState({ file: false, view: false, tools: false, compare: false, about: false });
  const [recentFiles, setRecentFiles] = useState([]);
  const [isCommandsOpen, setIsCommandsOpen] = useState(false);
  const [isBatchRenderOpen, setIsBatchRenderOpen] = useState(false);
  const [existingPng, setExistingPng] = useState('overwrite');
  const [batchModelKind, setBatchModelKind] = useState('g1m');
  const dropdownRefs = useRef({ file: null, view: null, tools: null, compare: null, about: null });

  const closeMenu = () => {
    setShowDropdown({ file: false, view: false, tools: false, compare: false, about: false });
  };


  //Buttons functions
  const handleOpenFileClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
  };
  const handleOpenFolderClick = (event) => {
    event.stopPropagation();
    closeMenu();
    openFolderContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
  };
  const handleOpenInternalSarcFile = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    if (activeTab === 'SARC') {
      editInternalSarcFile(selectedPath.path, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
    } else {
      setStatusText("Switch to SARC tab to edit files");
    }
  };
  const handleSaveClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    if (!isFileTypeSaveable(activeDocument?.fileType)) return;
    saveFileClick(setStatusText, activeTab, setpaths, editorRef, setSavingFile, documentSnapshots);
  };
  const handleSaveAsClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    if (!isFileTypeSaveable(activeDocument?.fileType)) return;
    saveAsFileClick(setStatusText, activeTab, setpaths, editorRef, setSavingFile, documentSnapshots);
  };

  const handleDownloadMiiGlb = (event) => downloadMiiGlb({
    event,
    closeMenu,
    activeDocument,
    documents,
    setStatusText,
    openFile: (path) => OpenFileFromPath(path, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent),
  });

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
  const handleAddFolderClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    addFilesFromDirRecursivelyToRoot(setStatusText, setpaths);
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
    extractRootFolderClick(setStatusText);
  }

  const handleBatchRender = async (event) => {
    event.stopPropagation();
    closeMenu();
    setIsBatchRenderOpen(true);
  };

  const startBatchRender = async () => {
    const sourceRoot = await invoke('open_dir_dialog', { title: 'Select folder with 3d files' });
    if (!sourceRoot) return;
    const outputRoot = await invoke('open_dir_dialog', { title: 'Select output folder for renders' });
    if (!outputRoot) return;
    setIsBatchRenderOpen(false);
    window.dispatchEvent(new CustomEvent('totkbits:batch-render', {
      detail: { sourceRoot, outputRoot, existingPng, modelKind: batchModelKind },
    }));
  };

  const handleCompareFileInternalWithVanila = async (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    try {
      if (activeTab === 'SARC') {
        await compareInternalFileWithOVanila(selectedPath.path, setStatusText, setActiveTab, setCompareData);
      } else if (activeTab === 'YAML') {
        //empty internal path, irrelevant
        await compareInternalFileWithOVanilaMonaco(setStatusText, setActiveTab, setCompareData, editorRef, setLabelTextDisplay);
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
      await compareFilesByDecision(setStatusText, setActiveTab, setCompareData, editorRef, isFromDisk, setLabelTextDisplay);

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
    const allPaths = paths.all_paths || paths.paths;
    setpaths({ ...paths, paths: allPaths, all_paths: allPaths });
    setStatusText(`Showing all files (${allPaths.length})`);
  }

  const handleShowAddedClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    // console.log(backupPaths.paths.length);
    // if (backupPaths.paths.length === 0) {
    //   setBackupPaths(paths);
    // }
    const allPaths = paths.all_paths || paths.paths;
    setpaths({ ...paths, paths: paths.added_paths, all_paths: allPaths });
    setStatusText(`Showing only added files (${paths.added_paths.length})`);
  }

  const handleShowModdedClick = (event) => {
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();
    // if (backupPaths.paths.length === 0) {
    //   setBackupPaths(paths);
    // }
    const allPaths = paths.all_paths || paths.paths;
    setpaths({ ...paths, paths: paths.modded_paths, all_paths: allPaths });
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
    clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery, documentSnapshots);
  }
  const handleEditOptions = (event) => {
    console.log("Edit options clicked");
    event.stopPropagation(); // Prevent click event from reaching parent
    closeMenu();

    // Settings owns its own modal state; keep the add/rename dialog closed.
    setIsModalOpen(false);
    setIsOptionsOpen(!isOptionsOpen);
    console.log("Config options open: ", isOptionsOpen);
  }

  const handlePhysicsMerge = async (event) => {
    event.stopPropagation();
    closeMenu();
    try {
      const [bphcl, hkcl] = await Promise.all([
        invoke('list_open_bphcl_documents'),
        invoke('list_open_hkcl_documents'),
      ]);
      if (bphcl.length + hkcl.length >= 2) {
        setPhysicsMergeReturnTab(activeTab);
        setActiveTab('PHYSICS_MERGE');
        setStatusText('Select HKCL or BPHCL nodes to merge');
      } else {
        setStatusText('ERROR: Open at least two HKCL or BPHCL documents before using Physics Merge');
      }
    } catch (error) {
      setStatusText(`ERROR: ${error}`);
    }
  };

  const toggleDropdown = (menu) => {
    setShowDropdown(prevState => ({
      ...{ file: false, view: false, tools: false, compare: false, about: false }, // Reset all to false
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

  useEffect(() => {
    invoke('get_recent_files').then(setRecentFiles).catch(() => setRecentFiles([]));
    const updateRecentFiles = (event) => setRecentFiles(event.detail || []);
    window.addEventListener('totkbits:recent-files-changed', updateRecentFiles);
    return () => window.removeEventListener('totkbits:recent-files-changed', updateRecentFiles);
  }, []);

  useEffect(() => {
    const refresh = () => invoke('get_aoc_model_catalog')
      .then(setAocModelCatalog)
      .catch(() => setAocModelCatalog(null));
    refresh();
    window.addEventListener('totkbits:aoc-config-changed', refresh);
    return () => window.removeEventListener('totkbits:aoc-config-changed', refresh);
  }, [setAocModelCatalog]);

  const handleOpenAocModels = async (event) => {
    event.stopPropagation();
    closeMenu();
    const { created } = openUtilityDocument('AOC models', 'AOC_MODELS');
    if (created) await new Promise((resolve) => requestAnimationFrame(resolve));
    setActiveTab('AOC_MODELS');
    setStatusText('Search Age of Calamity models by hash or name');
  };
  const iconSize = '20px';
  const blankIcon = 'menu/blank.png';
  const isSaveEnabled = isFileTypeSaveable(activeDocument?.fileType);

  const fileMenuItems = [
    { label: 'Open file', onClick: handleOpenFileClick, icon: 'file.png', shortcut: '' },
    {
      label: 'Open recent',
      icon: 'open_recent.png',
      shortcut: '',
      children: recentFiles.map((path) => ({
        label: path,
        title: path,
        onClick: async (event) => {
          event.stopPropagation();
          closeMenu();
          await OpenFileFromPath(path, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
        },
      })),
    },
    { label: 'Open folder', onClick: handleOpenFolderClick, icon: 'dir_opened.png', shortcut: '' },
    { label: 'AOC model', onClick: handleOpenAocModels, icon: 'menu/aoc_logo.png', shortcut: '', condition: aocModelCatalog !== null },
    { label: 'Save', onClick: handleSaveClick, icon: 'menu/save.png', shortcut: '', condition: isSaveEnabled },
    { label: 'Save as', onClick: handleSaveAsClick, icon: 'menu/save_as.png', shortcut: '', condition: isSaveEnabled },
    { label: 'Close all', onClick: handleCloseAllFilesClick, icon: 'menu/closeall.png', shortcut: '' },
    { label: 'Settings', onClick: handleEditOptions, icon: 'menu/edit_config.png', shortcut: '' },
    { label: 'Restart', onClick: restartAppClick, icon: 'menu/restart.png', shortcut: '' },
    { label: 'Exit', onClick: useExitApp, icon: 'menu/exit.png', shortcut: '' }
  ];
  const isSarcOpened = paths.paths.length > 0 && activeTab === "SARC";
  const isInternalFileSelected = isSarcOpened && selectedPath.path !== '' && selectedPath.isfile;
  const toolsMenuItems = [
    { label: 'Download GLB', onClick: handleDownloadMiiGlb, icon: blankIcon, shortcut: '', condition: activeDocument?.fileType === 'MII' },
    { label: 'Batch render', onClick: handleBatchRender, icon: blankIcon, shortcut: '', condition: true },
    { label: 'Physics merge', onClick: handlePhysicsMerge, icon: blankIcon, shortcut: '', condition: true },
    { label: 'Add file', onClick: handleAddClick, icon: 'menu/add.png', shortcut: '', condition: isSarcOpened },
    { label: 'Add folder', onClick: handleAddFolderClick, icon: 'menu/add_folder.png', shortcut: '', condition: isSarcOpened },
    { label: 'Extract sarc contents', onClick: handleExtractOpenedSarc, icon: 'context_menu/extract_all.png', shortcut: '', condition: isSarcOpened },
    { label: 'Search in sarc', onClick: handleSearchClick, icon: 'menu/lupa.png', shortcut: '', condition: isSarcOpened },
    { label: 'Clear search', onClick: handleClearSearchTextInSarc, icon: 'menu/clear_search.png', shortcut: '', condition: searchInSarcQuery.length > 0 },
    { label: 'Edit', onClick: handleOpenInternalSarcFile, icon: 'context_menu/edit.png', shortcut: '', condition: isInternalFileSelected },
    { label: 'Extract file', onClick: handleExtractClick, icon: 'context_menu/extract.png', shortcut: '', condition: isInternalFileSelected },
    // { label: 'Show all', onClick: handleShowAllClick, icon: blankIcon, shortcut: '', condition: paths.added_paths.length > 0 || paths.modded_paths.length > 0 },
    // { label: 'Show added', onClick: handleShowAddedClick, icon: blankIcon, shortcut: '', condition: paths.added_paths.length > 0 },
    // { label: 'Show modded', onClick: handleShowModdedClick, icon: blankIcon, shortcut: '', condition: paths.modded_paths.length > 0 }
  ];
  const isToolsMenuVisible = toolsMenuItems.some(item => item.condition);
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
        <div className="menu-items">
          <div className="menu-item" onClick={() => toggleDropdown('file')} ref={el => dropdownRefs.current.file = el}>
            File
            <div className="dropdown-content" style={{ display: showDropdown.file ? 'block' : 'none' }}>
              {fileMenuItems.filter(item => item.condition !== false).map((item, id) => (
                <div key={id} className={item.children ? 'menu-submenu-host' : undefined}>
                  <li
                    className="menu-item"
                    onClick={item.onClick}
                    style={menuItemStyle}
                  >
                    <div style={menuDivStyle}>
                      <img src={item.icon} alt={item.label} style={menuItemImgStyle} />
                      {item.label}
                    </div>
                    <span style={menuSpanStyle}>{item.shortcut}</span>
                    {item.children && <span className="menu-submenu-arrow" aria-hidden="true">▶</span>}
                  </li>
                  {item.children && (
                    <ul className="menu-submenu">
                      {item.children.length === 0 ? (
                        <li className="menu-submenu-item menu-submenu-empty">No recent files</li>
                      ) : item.children.map((child) => (
                        <li
                          key={child.title}
                          className="menu-submenu-item"
                          title={child.title}
                          onClick={child.onClick}
                        >
                          {child.label}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
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
          {isToolsMenuVisible && (
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
          <div className="menu-item" onClick={() => toggleDropdown('about')} ref={el => dropdownRefs.current.about = el}>
            About
            <div className="dropdown-content" style={{ display: showDropdown.about ? 'block' : 'none' }}>
              <li className="menu-item" style={menuItemStyle} onClick={(event) => {
                event.stopPropagation();
                closeMenu();
                setIsCommandsOpen(true);
              }}>
                <div style={menuDivStyle}><img src={blankIcon} alt="Commands" style={menuItemImgStyle} />Commands</div>
              </li>
            </div>
          </div>
        </div>
        <div className="menu-right-content">
          <div className="menu-file-metadata">{fileMetadata}</div>
          {updateButton}
        </div>
      </div>
      <CommandsHelp isOpen={isCommandsOpen} onClose={() => setIsCommandsOpen(false)} />
      {isBatchRenderOpen && <div className="batch-render-options-overlay" role="dialog" aria-modal="true" aria-labelledby="batch-render-options-title">
        <section className="batch-render-options">
          <h2 id="batch-render-options-title">Batch render PNG</h2>
          <label>When PNG file exists
            <select value={existingPng} onChange={(event) => setExistingPng(event.target.value)}>
              <option value="skip">Skip</option>
              <option value="overwrite">Overwrite</option>
            </select>
          </label>
          <label>Models
            <select value={batchModelKind} onChange={(event) => setBatchModelKind(event.target.value)}>
              <option value="all">All</option>
              <option value="g1m">G1M</option>
              <option value="bfres">BFRES</option>
            </select>
          </label>
          <footer>
            <button type="button" onClick={() => setIsBatchRenderOpen(false)}>Cancel</button>
            <button type="button" onClick={startBatchRender}>Render</button>
          </footer>
        </section>
      </div>}
    </div>
  );


}

function MenuBarDisplayWithUpdateButton() {
  const {
    updateState, setStatusText, settings
  } = useEditorContext();
  const handleUpdateClick = async (event) => {
    if (!updateState.isUpdateNeeded) { return null; }
    try {
      await open('https://github.com/SolidLink95/TotkBits/releases/latest');
    } catch (error) {
      console.error('Failed to open release page: ', error);
      setStatusText('ERROR: Failed to open release page');
    }
  }
  const iconSize = '28px';
  const isUp = updateState.isUpdateNeeded;
  const SHOW_UPDATE_BUTTON = true;
  return (
    <div style={{
      display: 'flex',
      justifyContent: 'space-between',
      backgroundColor: '#333',
      // fontWeight: 'bold',
    }}>
      <MenuBarDisplay updateButton={SHOW_UPDATE_BUTTON ? <ImageButton
        key={isUp ? 'UpdateAvailableButton' : 'NoUpdateButton'}
        src={isUp ? 'update.png' : 'noupdate.png'}
        alt={
          isUp
            ? `Update to ${updateState.latestVersion}`
            : 'Totkbits is up to date'
        }
        onClick={handleUpdateClick}
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
      /> : null} />
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        {settings.zstd_msg && <div style={{ padding: '2px', color: 'yellow' }}>{settings.zstd_msg}</div>}
      </div>
    </div>
  );
}

export { MenuBarDisplay, MenuBarDisplayWithUpdateButton };
