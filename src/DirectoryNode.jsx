import React, { useState } from 'react';
import { extractRootFolderClick, extractFolderClick, editInternalSarcFile, replaceInternalFileClick, removeInternalFileClick, addInternalFileToDir, extractFileClick, addEmptyByml,addFilesFromDirRecursively } from './ButtonClicks';
import { useEditorContext } from './StateManager';
import {compareInternalFileWithOVanila} from './Comparer';

const dirOpened = `dir_opened.png`;
const dirClosed = `dir_closed.png`;
const fileIcon = `file.png`;
const iconSize = '20px';

const ContextMenu = ({ x, y, onClose, actions, settings }) => {
  return (
    <ul
      className="context-menu"
      style={{
        fontSize: settings.contextMenuFontSize,
        position: 'absolute',
        top: y,
        left: x,
        listStyleType: 'none',
        padding: '6px',
        boxShadow: '0 2px 10px rgba(0,0,0,0.2)',
        zIndex: 100,
      }}
      onMouseLeave={onClose}
    >
      {actions.map((action, index) => (
        <li
          key={index}
          className="context-menu-item"
          onClick={() => action.method()}
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}
        >
          <div style={{ display: 'flex', alignItems: 'center' }}>
            <img src={action.icon} alt={action.label} style={{ marginRight: '10px', width: '20px', height: '20px' }} />
            {action.label}
          </div>
          <span style={{ marginLeft: '10px', color: '#bcbcbc' }}>{action.shortcut} </span>
        </li>
      ))}
    </ul>
  );
};
//{ editorRef, updateEditorContent, setStatusText, activeTab, setActiveTab, setLabelTextDisplay, setpaths, selectedPath, changeModal }
const DirectoryNode = ({ node, name, path, onContextMenu, sarcPaths, selected, onSelect }) => {
  const {
    settings, setSettings,
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal, setCompareData, setInternalSarcPath
  } = useEditorContext();

  const [isCollapsed, setIsCollapsed] = useState(true);
  const [contextMenu, setContextMenu] = useState({ visible: false, x: 0, y: 0 });
  const isFile = node === null;
  const fullPath = path ? `${path}/${name}` : name;
  // const endian = "LE";
  const isSelected = selected === fullPath;

  const handleDoubleClick = (e) => {
    e.stopPropagation(); // Prevent the click from bubbling up to parent elements
    console.log(`Double-clicked on directory: ${fullPath}`);
    handleOpenInternalSarcFile();
    // Add your custom double-click logic here
  };

  const handleSelect = (e) => {
    e.stopPropagation(); // This stops the event from bubbling up further
    console.log(`Selected: ${fullPath}`);
    onSelect(fullPath, isFile); // Pass the fullPath to the onSelect function
  };

  const handleAddClick = () => {
    closeContextMenu();
    setIsAddPrompt(true);
    setIsModalOpen(true);
  }

  const handleExtractInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      extractFileClick({ path: fullPath }, setStatusText);
    }
  };
  const handleExtractInternalSarcFolder = () => {
    closeContextMenu();
    if (!isFile) {
      console.log("Extracting folder:", fullPath);
      extractFolderClick(fullPath, setStatusText);
    }
  };

  const handleOpenInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      editInternalSarcFile(fullPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent);
    }
  };
  const handleCompareInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      compareInternalFileWithOVanila(selectedPath.path, setStatusText, setActiveTab, setCompareData);
    }
  };
  const handleRemoveInternalSarcFile = () => {
    closeContextMenu();
    removeInternalFileClick(fullPath, setStatusText, setpaths);
  };
  const handleReplaceInternalSarcFile = () => {
    closeContextMenu();
    replaceInternalFileClick(fullPath, setStatusText, setpaths);
  };
  const handleAddInternalSarcFileToDir = () => {
    closeContextMenu();
    addInternalFileToDir(fullPath, setStatusText, setpaths);
  };  
  const handleAddFilesFromDirRecursively = () => {
    closeContextMenu();
    addFilesFromDirRecursively(fullPath, setStatusText, setpaths);
  };
  const handleAddEmptyByml = () => {
    closeContextMenu();
    addEmptyByml(fullPath, setStatusText, setpaths);
  };

  const handleRenameInternalSarcFile = () => {
    closeContextMenu();
    if (isFile) {
      setRenamePromptMessage({ message: "Rename the internal SARC file:", path: name });
    } else {
      setRenamePromptMessage({ message: "Rename the internal SARC directory:", path: name });
    }
    setIsAddPrompt(false);
    setIsModalOpen(true);

  }
  const handlePathToClipboard = (text) => {
    closeContextMenu();
    navigator.clipboard.writeText(text).then(() => {
      console.log('Text copied to clipboard');
    }).catch(err => {
      console.error('Failed to copy text: ', err);
    });
    setStatusText(`Copied to clipboard`);
  }


  const nodeStyle = {
    borderRadius: '5px',
    width: '95%',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    color: isSelected && isFile ? 'white' : 'white',
    backgroundColor: //isFile ?
      isSelected ?
        sarcPaths.added_paths.includes(fullPath) ? '#2D8589' :
          sarcPaths.modded_paths.includes(fullPath) ? '#B78F00' :
            '#303030' :
        sarcPaths.added_paths.includes(fullPath) ? '#1E595B' :
          sarcPaths.modded_paths.includes(fullPath) ? '#826C00' :
            'transparent'// :
    // 'transparent'


  };

  const toggleCollapse = () => {
    setIsCollapsed(!isCollapsed);
    closeContextMenu();
  };

  const handleIconClick = (e) => {
    if (!isFile) {
      toggleCollapse();
    }
    e.stopPropagation();
  };

  const handleIconContextMenu = (e) => {
    e.preventDefault();
    let offsetX = window.scrollX || document.documentElement.scrollLeft;
    let offsetY = window.scrollY || document.documentElement.scrollTop;
    offsetX = offsetX - 5;
    offsetY = offsetY - 5;
    // If the tree container itself has a scroll, add this offset too
    // You need to replace '.tree-container' with the actual selector of your container
    const treeContainer = document.querySelector('.directory-tree');
    if (treeContainer) {
      offsetX += treeContainer.scrollLeft - treeContainer.getBoundingClientRect().left;
      offsetY += treeContainer.scrollTop - treeContainer.getBoundingClientRect().top;
    }
    const height = 350;
    let yval = e.clientY + offsetY + height;
    if (yval > window.innerHeight) {
      yval = window.innerHeight - height;
    }
    yval =  e.clientY + offsetY;
    // console.log(parseInt(yval, 10), parseInt(yval, 10)+height, window.innerHeight, parseInt(yval, 10)+height-window.innerHeight);
    setContextMenu({
      visible: true,
      x: e.clientX + offsetX,
      y: yval 
      // y: e.clientY + offsetY > window.innerHeight ? window.innerHeight - height : yval,
    });
    e.stopPropagation();
    onContextMenu && onContextMenu(fullPath);
  };

  const closeContextMenu = () => {
    setContextMenu({ visible: false, x: 0, y: 0 });
  };

  const contextMenuActions = isFile ? [
    { label: 'Edit', method: handleOpenInternalSarcFile, icon: 'context_menu/edit.png', shortcut: 'F3' },
    { label: 'Compare', method: handleCompareInternalSarcFile, icon: 'context_menu/compare.png', shortcut: '' },
    { label: 'Extract', method: handleExtractInternalSarcFile, icon: 'context_menu/extract.png', shortcut: 'Ctrl+E' },
    { label: 'Replace', method: handleReplaceInternalSarcFile, icon: 'context_menu/replace.png', shortcut: 'Ctrl+R' },
    { label: 'Delete', method: handleRemoveInternalSarcFile, icon: 'context_menu/remove.png', shortcut: '' },
    { label: 'Rename', method: handleRenameInternalSarcFile, icon: 'context_menu/rename.png', shortcut: '' },
    { label: 'Copy path', method: () => handlePathToClipboard(fullPath), icon: 'context_menu/copy.png', shortcut: '' },
    { label: 'Close', method: () => closeContextMenu(), icon: 'context_menu/close.png', shortcut: '' },
  ] : [
    { label: 'Add file', method: handleAddInternalSarcFileToDir, icon: 'context_menu/add_file.png', shortcut: '' },
    { label: 'Add folder', method: handleAddFilesFromDirRecursively, icon: 'context_menu/add_dir.png', shortcut: '' },
    { label: 'Extract', method: handleExtractInternalSarcFolder, icon: 'context_menu/extract.png', shortcut: 'Ctrl+E' },
    { label: 'New byml', method: handleAddEmptyByml, icon: 'context_menu/byml.png', shortcut: '' },
    { label: 'Delete', method: handleRemoveInternalSarcFile, icon: 'context_menu/remove.png', shortcut: '' },
    { label: 'Rename', method: handleRenameInternalSarcFile, icon: 'context_menu/rename.png', shortcut: '' },
    { label: 'Close', method: () => closeContextMenu(), icon: 'context_menu/close.png', shortcut: '' },
  ];

  return (
    <li onContextMenu={onContextMenu} onClick={handleSelect}>
      <div style={nodeStyle}
        onContextMenu={handleIconContextMenu}
        onDoubleClick={handleDoubleClick}>
        <img
          src={isFile ? fileIcon : isCollapsed ? dirClosed : dirOpened}
          alt={name}
          style={{ marginRight: '5px', width: iconSize, height: iconSize }}
          onClick={handleIconClick}
          onContextMenu={handleIconContextMenu}
        />
        <span onClick={toggleCollapse}>{name}</span>
      </div>
      {!isFile && (
        <div className={`node-children ${isCollapsed ? 'collapsed' : 'expanded'}`}>
          <ul style={{ marginLeft: '40px', listStyleType: 'none', padding: 0 }}>
            {Object.entries(node).map(([key, value]) => (
              <DirectoryNode
                key={key}
                node={value}
                name={key}
                path={fullPath}
                onContextMenu={onContextMenu}
                sarcPaths={sarcPaths}
                selected={selected} // Make sure this is passed correctly
                onSelect={onSelect} // Make sure this is passed correctly
              />
            ))}
          </ul>
        </div>
      )}
      {contextMenu.visible && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={closeContextMenu}
          actions={contextMenuActions}
          settings={settings}
        />
      )}
    </li>
  );
};


export default DirectoryNode;
