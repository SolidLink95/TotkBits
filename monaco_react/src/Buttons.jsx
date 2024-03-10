import React, { useCallback, useState } from 'react';
import { editInternalSarcFile, fetchAndSetEditorContent, saveAsFileClick, saveFileClick } from './ButtonClicks';

const button_size = '33px';

const ButtonsDisplay = ({ editorRef, updateEditorContent, setStatusText, activeTab, setActiveTab, setLabelTextDisplay, setpaths, selectedPath, changeModal }) => {
  //Buttons functions
  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const handleFetchContent = () => {
    fetchAndSetEditorContent(setStatusText, setActiveTab, setLabelTextDisplay, setpaths, updateEditorContent);
  };
  const handleOpenInternalSarcFile = () => {
    editInternalSarcFile(selectedPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent);
  };
  const handleSaveClick = () => {
    saveFileClick(setStatusText, activeTab, setpaths, editorRef);
  };
  const handleSaveAsClick = () => {
    saveAsFileClick(setStatusText, activeTab, setpaths, editorRef);
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
    { src: 'open.png', alt: 'Open', onClick: handleFetchContent, title: 'Open (Ctrl+O)' },
    { src: 'save.png', alt: 'Save', onClick: handleSaveClick, title: 'Save (Ctrl+S)' },
    { src: 'save_as.png', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
    { src: 'edit.png', alt: 'edit', onClick: handleOpenInternalSarcFile, title: 'Edit (Ctrl+E)' },
    { src: 'add_sarc.png', alt: 'add', onClick: changeModal, title: 'Add' },
    { src: 'extract.png', alt: 'extract', onClick: () => console.log('extract clicked'), title: 'Extract' },
    // { src: 'lupa.png', alt: 'find', onClick: () => console.log('find clicked'), title: 'Find (Ctrl+F)' },
    // { src: 'replace.png', alt: 'replace', onClick: () => console.log('replace clicked'), title: 'Replace (Ctrl+H)' },
  ] :[
    { src: 'open.png', alt: 'Open', onClick: handleFetchContent, title: 'Open (Ctrl+O)' },
    { src: 'save.png', alt: 'Save', onClick: handleSaveClick, title: 'Save (Ctrl+S)' },
    { src: 'save_as.png', alt: 'save_as', onClick: handleSaveAsClick, title: 'Save as' },
    // { src: 'edit.png', alt: 'edit', onClick: handleOpenInternalSarcFile, title: 'Edit (Ctrl+E)' },
    // { src: 'add_sarc.png', alt: 'add', onClick: () => console.log('add clicked'), title: 'Add' },
    // { src: 'extract.png', alt: 'extract', onClick: () => console.log('extract clicked'), title: 'Extract' },
    { src: 'back.png', alt: 'back', onClick: undoInEditor, title: 'Undo (Ctrl+Z)' },
    { src: 'forward.png', alt: 'forward', onClick: redoInEditor, title: 'Redo (Ctrl+Shift+Z)' },
    { src: 'lupa.png', alt: 'find', onClick: triggerSearchInEditor, title: 'Find (Ctrl+F)' },
    { src: 'replace.png', alt: 'replace', onClick: triggerReplaceInEditor, title: 'Replace (Ctrl+H)' },
    ]
  ;


  return (
    <div className="buttons-container">
      {imageButtonsData.map((button, index) => (
        <ImageButton key={index} src={button.src} alt={button.alt} onClick={button.onClick} title={button.title} style={button.alt === 'back' ? { marginLeft: '10px' } : {}} />
      ))}
    </div>
  );
};

export default ButtonsDisplay;