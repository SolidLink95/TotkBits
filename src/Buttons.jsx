import React, { useCallback, useEffect, useRef } from 'react';
import {clearSearchInSarcClick, searchTextInSarcClick,editInternalSarcFile, extractFileClick, fetchAndSetEditorContent, saveAsFileClick, saveFileClick } from './ButtonClicks';
import { useEditorContext } from './StateManager';

const button_size = '33px';

const ButtonsDisplay = () => {
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
  const activeTabRef = useRef(activeTab);

  //Buttons functions
  const handleOpenFileClick = () => {
    fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
  };
  const handleOpenInternalSarcFile = () => {
    editInternalSarcFile(selectedPath.path, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent);
  };
  const handleSaveClick = () => {
    console.log(activeTabRef.current, activeTab);
    saveFileClick(setStatusText, activeTabRef.current, setpaths, editorRef);
  };
  const handleSaveAsClick = () => {
    saveAsFileClick(setStatusText, activeTabRef.current, setpaths, editorRef);
  };
  const handleClearSarcSearch = () => {
    clearSearchInSarcClick(setpaths, setStatusText, setSearchInSarcQuery);
  };
  const handleAddClick = () => {
    setIsAddPrompt(true);
    setIsModalOpen(true);
  }
  const handleSearchClick = () => {
    setIsSearchInSarcOpened(!isSearchInSarcOpened);
  };

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

  const imageButtonsData = activeTab === "SARC" ? [
    { src: 'open.png', alt: 'Open', onClick: handleOpenFileClick, title: 'Open (Ctrl+O)' },
    { src: 'save.png', alt: 'Save', onClick: handleSaveClick, title: 'Save (Ctrl+S)' },
    { src: 'save_as.png', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
    { src: 'edit.png', alt: 'edit', onClick: handleOpenInternalSarcFile, title: 'Edit (Ctrl+E)' },
    { src: 'add_sarc.png', alt: 'add', onClick: handleAddClick, title: 'Add' },
    { src: 'extract.png', alt: 'extract', onClick: () => extractFileClick(selectedPath, setStatusText), title: 'Extract' },
    { src: 'lupa.png', alt: 'find', onClick: handleSearchClick, title: 'Search in sarc' },
  ] : activeTab === "YAML" ? [
    { src: 'open.png', alt: 'Open', onClick: handleOpenFileClick, title: 'Open (Ctrl+O)' },
    { src: 'save.png', alt: 'Save', onClick: handleSaveClick, title: 'Save (Ctrl+S)' },
    { src: 'save_as.png', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
    { src: 'back.png', alt: 'back', onClick: undoInEditor, title: 'Undo (Ctrl+Z)' },
    { src: 'forward.png', alt: 'forward', onClick: redoInEditor, title: 'Redo (Ctrl+Shift+Z)' },
    { src: 'lupa.png', alt: 'find', onClick: triggerSearchInEditor, title: 'Find (Ctrl+F)' },
    { src: 'replace.png', alt: 'replace', onClick: triggerReplaceInEditor, title: 'Replace (Ctrl+H)' },
  ] : [
    { src: 'open.png', alt: 'Open', onClick: handleOpenFileClick, title: 'Open (Ctrl+O)' },
    { src: 'save.png', alt: 'Save', onClick: handleSaveClick, title: 'Save (Ctrl+S)' },
    { src: 'save_as.png', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
    // { src: 'back.png', alt: 'back', onClick: undoInEditor, title: 'Undo (Ctrl+Z)' },
    // { src: 'forward.png', alt: 'forward', onClick: redoInEditor, title: 'Redo (Ctrl+Shift+Z)' },
    // { src: 'lupa.png', alt: 'find', onClick: triggerSearchInEditor, title: 'Find (Ctrl+F)' },
    // { src: 'replace.png', alt: 'replace', onClick: triggerReplaceInEditor, title: 'Replace (Ctrl+H)' },
  ]
    ;

    
  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);

  useEffect(() => {//handle mouse and keyboards events
    const handleContextMenu = (event) => {
      event.preventDefault();//prevent browser's default context menu
      //commented out in order to access "Inspect" feature
    };
    const handleKeyDown = (event) => {
      const functionKeys = ['F1', 'F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8', 'F9', 'F10', 'F11', 'F12'];
      // Check if the pressed key is one of the function keys
      if (functionKeys.includes(event.code)) {
        event.preventDefault(); // Prevent the default action
      }
      // Check if Ctrl or Command (for macOS) is pressed
      if (!event.ctrlKey && !event.metaKey) return;
      if (event.ctrlKey && event.shiftKey && event.keyCode === 83) {
        event.preventDefault(); //prevent "Screenshot" feature for browser
        handleSaveAsClick();
      }

      switch (event.key) {
        case 'o': // Ctrl+O
          event.preventDefault(); // Prevent the browser's default action
          handleOpenFileClick();
          break;
        case 's': // Ctrl+S
          event.preventDefault();
          handleSaveClick();
          break;
        case 'e': // Ctrl+E,
          if (activeTabRef.current === 'SARC') {
            event.preventDefault();
            extractFileClick(selectedPath, setStatusText);
          }
          break;
        case 'f': // Ctrl+F: prevent the browser's default action
          event.preventDefault();
          break;
      }
    };

    // Add event listener for keydown
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('contextmenu', handleContextMenu);

    // Clean up the event listener when the component unmounts
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('contextmenu', handleContextMenu);
    };
  }, []); // Pass an empty dependency array to ensure this effect runs only once after the initial render
  const isClearSearchShown = activeTab == "SARC" && searchInSarcQuery.length > 0 && !isSearchInSarcOpened;

  return (
    <div className="buttons-container">
      {imageButtonsData.map((button, index) => (
        <ImageButton key={index} src={button.src} alt={button.alt} onClick={button.onClick} title={button.title} style={button.alt === 'back' || button.alt === "find" ? { marginLeft: '10px' } : {}} />
      ))}
      {isClearSearchShown && <button className="modal-footer-button" onClick={handleClearSarcSearch} title="Clear active search" >Clear search</button>}
    </div>
  );
};

export default ButtonsDisplay;