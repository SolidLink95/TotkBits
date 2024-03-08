import React from 'react';
import { invoke } from '@tauri-apps/api/tauri'; // Import Tauri invoke method

function ButtonsDisplay({ updateEditorContent }) {
  const fetchAndSetEditorContent = async () => {
    try {
      const content = await invoke('send_text_to_frontend'); // Use the command name you defined in Rust
      updateEditorContent(content); // This line calls the function passed down as a prop
      console.log(content);
    } catch (error) {
      console.error('Failed to fetch editor content from Rust backend:', error);
    }
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
          width: '40px', // Define your desired width
          height: '40px', // Define your desired height 
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


  const imageButtonsData = [
    { src: 'open.png', alt: 'Open', onClick: () => console.log('Open clicked'), title: 'Open (Ctrl+O)' },
    { src: 'save.png', alt: 'Save', onClick: () => console.log('Save clicked'), title: 'Save (Ctrl+S)' },
    { src: 'save_as.png', alt: 'save_as', onClick: () => console.log('save_as clicked'), title: 'Save as' },
    { src: 'edit.png', alt: 'edit', onClick: () => console.log('edit clicked'), title: 'Edit (Ctrl+E)' },
    { src: 'add_sarc.png', alt: 'add', onClick: () => console.log('add clicked'), title: 'Add' },
    { src: 'extract.png', alt: 'extract', onClick: () => console.log('extract clicked'), title: 'Extract' },
    // { src: 'zoomin.png', alt: 'zoomin', onClick: () => console.log('zoomin clicked') },
    //{ src: 'zoomout.png', alt: 'zoomout', onClick: () => console.log('zoomout clicked') },
    { src: 'lupa.png', alt: 'find', onClick: () => console.log('find clicked'), title: 'Find (Ctrl+F)' },
    { src: 'replace.png', alt: 'replace', onClick: () => console.log('replace clicked'), title: 'Replace (Ctrl+H)' },
    { src: 'add.png', alt: 'TEST1', onClick: fetchAndSetEditorContent, title: 'send some example string to monace' },
    { src: 'add.png', alt: 'TEST2', onClick: () => console.log('TEST2'), title: 'TEST2' },
  ];


  return (
    <div className="buttons-container">
      {imageButtonsData.map((button, index) => (
        <ImageButton key={index} src={button.src} alt={button.alt} onClick={button.onClick} title={button.title} style={button.alt === 'find' ? { marginLeft: '10px' } : {}} />
      ))}
    </div>
  );
}

export default ButtonsDisplay;