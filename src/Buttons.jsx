import React, { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { removeInternalFileClick, replaceInternalFileClick, clearSearchInSarcClick, searchTextInSarcClick, editInternalSarcFile, extractFileClick, fetchAndSetEditorContent, saveAsFileClick, saveFileClick } from './ButtonClicks';
import { getDocumentsSnapshot, subscribeDocuments } from './DocumentState';
import { isFileTypeSaveable } from './FileTypes';
import { useEditorContext } from './StateManager';





const button_size = '33px';



function ImageButton({ src, onClick, alt, title, style }) {
  // Apply both the background image and styles directly to the button
  return (
    <button
      onClick={onClick}
      className='button'
      style={{
        backgroundImage: `url(${src})`,
        backgroundSize: 'cover', // Cover the entire area of the button
        backgroundPosition: 'center', // Center the background image
        width: button_size, // Define your desired width
        height: button_size, // Define your desired height 
        display: 'flex', // Ensure the button content (if any) is centered
        justifyContent: 'left', // Center horizontally
        alignItems: 'left', // Center vertically
        ...style // Spread additional styles here
      }}
      aria-label={alt} // Accessibility label for the button if the image fails to load or for screen readers
      title={title}
    >
    </button>
  );
}

const ButtonsDisplay = () => {
  const { documents, activeDocumentId } = useSyncExternalStore(subscribeDocuments, getDocumentsSnapshot);
  const activeDocument = documents.find((document) => document.id === activeDocumentId);
  const isSaveEnabled = isFileTypeSaveable(activeDocument?.fileType);
  const showCloseButton = documents.length > 1 || (documents.length === 1 && !documents[0].clean);
  const {
    searchInSarcQuery, setSearchInSarcQuery,
    isSearchInSarcOpened, setIsSearchInSarcOpened,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, pathsFilters, setPathsFilters, isModalOpen, setIsModalOpen, updateEditorContent, changeModal,
    setSavingFile, documentSnapshots
  } = useEditorContext();

  const displayButtons = !['3D', 'IMAGE', 'AMTA', 'MODEL_BROWSER'].includes(activeTab);
  // console.log("Display buttons? ", displayButtons);
  const handlePathToClipboard = (text) => {
    navigator.clipboard.writeText(text).then(() => {
      console.log('Text copied to clipboard');
    }).catch(err => {
      console.error('Failed to copy text: ', err);
    });
    setStatusText(`Copied to clipboard`);
  }

  //Buttons functions
  const handleOpenFileClick = () => {
    fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
  };
  const handleOpenInternalSarcFile = () => {
    if (selectedPath.isfile) {
      editInternalSarcFile(selectedPath.path, setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
    }
  };

  const handleRemoveInternalElement = () => {
    removeInternalFileClick(selectedPath.path, setStatusText, setpaths);
  };

  const handleSaveClick = () => {
    if (!isSaveEnabled) return null;
    saveFileClick(setStatusText, activeTab, setpaths, editorRef, setSavingFile, documentSnapshots);
  };
  const handleSaveAsClick = () => {
    if (!isSaveEnabled) return null;
    saveAsFileClick(setStatusText, activeTab, setpaths, editorRef, setSavingFile, documentSnapshots);
  };
  const handleClearSarcSearch = () => {
    clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery, documentSnapshots);
  };
  const handleAddClick = () => {
    setIsAddPrompt(true);
    setIsModalOpen(true);
  }
  const handleSearchClick = () => {
    if (!Array.isArray(paths?.paths) || paths.paths.length === 0) return;
    setIsSearchInSarcOpened(!isSearchInSarcOpened);
  };
  const handleReplaceSarcNodeClick = () => {
    if (selectedPath.isfile) {
      console.log(selectedPath);
      replaceInternalFileClick(selectedPath.path, setStatusText, setpaths);
      setStatusText("Replaced file");
    } else {
      setStatusText("No file selected");
    }
  }


  const triggerSearchInEditor = useCallback(() => {
    if (editorRef.current) {
      editorRef.current.getAction('actions.find').run();
    }
  }, []);
  const triggerReplaceInEditor = useCallback(() => {
    if (editorRef.current) {
      editorRef.current.getAction('editor.action.startFindReplaceAction').run();
    }
  }, []);
  const undoInEditor = useCallback(() => {
    if (editorRef.current) {
      editorRef.current.trigger("source", "undo");
    }
  }, []);
  const redoInEditor = useCallback(() => {
    if (editorRef.current) {
      editorRef.current.trigger("source", "redo");
    }
  }, []);

  const imageButtonsData = activeTab === "AUDIO" ? [
    { src: 'open.webp', alt: 'Open', onClick: handleOpenFileClick, title: 'Open' },
    { src: 'save.webp', alt: 'Save', onClick: handleSaveClick, title: 'Save' },
    { src: 'save_as.webp', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
  ] : activeTab === "SARC" ? [
    { src: 'open.webp', alt: 'Open', onClick: handleOpenFileClick, title: 'Open' },
    { src: 'save.webp', alt: 'Save', onClick: handleSaveClick, title: 'Save' },
    { src: 'save_as.webp', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
    { src: 'edit.webp', alt: 'edit', onClick: handleOpenInternalSarcFile, title: 'Edit' },
    { src: 'add_sarc.webp', alt: 'add', onClick: handleAddClick, title: 'Add' },
    { src: 'extract.webp', alt: 'extract', onClick: () => extractFileClick(selectedPath, setStatusText), title: 'Extract' },
    { src: 'lupa.webp', alt: 'find', onClick: handleSearchClick, title: 'Search in sarc' },
  ] : activeTab === "YAML" ? [
    { src: 'open.webp', alt: 'Open', onClick: handleOpenFileClick, title: 'Open' },
    { src: 'save.webp', alt: 'Save', onClick: handleSaveClick, title: 'Save' },
    { src: 'save_as.webp', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
    { src: 'back.webp', alt: 'back', onClick: undoInEditor, title: 'Undo' },
    { src: 'forward.webp', alt: 'forward', onClick: redoInEditor, title: 'Redo' },
    { src: 'lupa.webp', alt: 'find', onClick: triggerSearchInEditor, title: 'Find' },
    { src: 'replace.webp', alt: 'replace', onClick: triggerReplaceInEditor, title: 'Replace' },
  ] : [
    { src: 'open.webp', alt: 'Open', onClick: handleOpenFileClick, title: 'Open' },
    { src: 'save.webp', alt: 'Save', onClick: handleSaveClick, title: 'Save' },
    { src: 'save_as.webp', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
  ]
    ;
  const handleFilterChange = (setPathsFilters, key, val) => {
    setPathsFilters((prevFilters) => {
      const allPaths = paths.all_paths || paths.paths;
      const restoreAllPaths = () => setpaths({
        ...paths,
        paths: allPaths,
        all_paths: allPaths,
      });
      const showAllFiles = () => ({
        showAll: true,
        showAdded: false,
        showModded: false
      });

      let newFilters = { ...prevFilters, [key]: val };

      switch (key) {
        case "showAll":
          restoreAllPaths();
          setStatusText(`Showing all files (${allPaths.length})`);
          newFilters = showAllFiles();
          break;

        case "showAdded":
          if (val) {
            setpaths({
              paths: paths.added_paths,
              added_paths: paths.added_paths,
              modded_paths: paths.modded_paths,
              nested_paths: paths.nested_paths || {},
              all_paths: allPaths
            });
            setStatusText(`Showing only added files (${paths.added_paths.length})`);
            newFilters = { showAll: false, showAdded: true, showModded: false };
          } else {
            restoreAllPaths();
            newFilters = showAllFiles();
          }
          break;

        case "showModded":
          if (val) {
            setpaths({
              paths: paths.modded_paths,
              added_paths: paths.added_paths,
              modded_paths: paths.modded_paths,
              nested_paths: paths.nested_paths || {},
              all_paths: allPaths
            });
            setStatusText(`Showing only modded files (${paths.modded_paths.length})`);
            newFilters = { showAll: false, showAdded: false, showModded: true };
          } else {
            restoreAllPaths();
            newFilters = showAllFiles();
          }
          break;

        default:
          setStatusText(`ERROR: Bad filters: ${prevFilters.showAll}, ${prevFilters.showAdded}, ${prevFilters.showModded}`);
          return newFilters;
      }

      console.log("Updated filters:", newFilters);
      return newFilters; // Return the new state
    });
  };

  const PathsFilterCheckboxes = () => {
    if (activeTab !== "SARC") return null;
    if (paths.paths.length == 0) return null;

    useEffect(() => {
      // console.log("Checkbox state updated:", pathsFilters);
    }, [pathsFilters]);
    const isAddedShown = paths.added_paths.length > 0;
    const isModdedShown = paths.modded_paths.length > 0;
    const isAllShown = isAddedShown || isModdedShown;
    if (!isAllShown) return null;
    const filters = [
      { key: "showAll", label: "All", var: pathsFilters.showAll, isShown: isAllShown },
      { key: "showAdded", label: "Added", var: pathsFilters.showAdded, isShown: isAddedShown  },
      { key: "showModded", label: "Modded", var: pathsFilters.showModded, isShown: isModdedShown  }
    ];
    return (
      <div >
        {filters.map((filter) => (
          filter.isShown ? <label style={{ paddingLeft: '5px' }}><input
            type="radio"
            checked={filter.var}
            onChange={(e) => handleFilterChange(setPathsFilters, filter.key, e.target.checked)}
          />
            {filter.label}
          </label> : null
        ))
        }

      </div>
    );
  };


  const isClearSearchShown = activeTab == "SARC" && searchInSarcQuery.length > 0 && !isSearchInSarcOpened;

  if (!displayButtons) return null;

  return (
    <div>
      <div className="buttons-container">
        {imageButtonsData.filter((button) => isSaveEnabled || (button.alt !== 'Save' && button.alt !== 'save_as')).map((button, index) => (
          <ImageButton
            key={index}
            src={button.src}
            alt={button.alt}
            onClick={button.onClick}
            title={button.title}
            style={button.alt === 'back' || button.alt === 'find' ? { marginLeft: '10px' } : {}}
          />
        ))}
        <PathsFilterCheckboxes />
        {isClearSearchShown && (
          <button
            className="modal-footer-button"
            onClick={handleClearSarcSearch}
            title="Clear active search"
          >
            Clear search
          </button>
        )}
        {showCloseButton && (
          <button
            className="toolbar-close-button"
            onClick={() => window.dispatchEvent(new CustomEvent('totkbits:close-active-document'))}
            title="Close current tab"
          >
            Close
          </button>
        )}
      </div>
    </div>
  );

};

export { ImageButton };
export default ButtonsDisplay;
