import React, { useState } from 'react';
import { editInternalSarcFile, replaceInternalFileClick, removeInternalFileClick, addInternalFileToDir } from './ButtonClicks';
import { useEditorContext } from './StateManager';

const dirOpened = `dir_opened.png`;
const dirClosed = `dir_closed.png`;
const fileIcon = `file.png`;
const iconSize = '20px';

const ContextMenu = ({ x, y, onClose, actions }) => {
  return (
    <ul
      className="context-menu"
      style={{
        position: 'absolute',
        top: y,
        left: x,
        listStyleType: 'none',
        padding: '6px',
        // border: '1px solid #ddd',
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
        >
          {action.label}
        </li>
      ))}
    </ul>
  );
};
//{ editorRef, updateEditorContent, setStatusText, activeTab, setActiveTab, setLabelTextDisplay, setpaths, selectedPath, changeModal }
const DirectoryNode = ({ node, name, path, onContextMenu, sarcPaths, selected, onSelect }) => {
  const {
    renamePromptMessage, setRenamePromptMessage,
    isAddPrompt, setIsAddPrompt,
    activeTab, setActiveTab,
    editorContainerRef, editorRef, editorValue, setEditorValue, lang, setLang,
    statusText, setStatusText, selectedPath, setSelectedPath, labelTextDisplay, setLabelTextDisplay,
    paths, setpaths, isModalOpen, setIsModalOpen, updateEditorContent, changeModal
  } = useEditorContext();

  const [isCollapsed, setIsCollapsed] = useState(true);
  const [contextMenu, setContextMenu] = useState({ visible: false, x: 0, y: 0 });
  const isFile = node === null;
  const fullPath = path ? `${path}/${name}` : name;
  // const endian = "LE";
  const isSelected = selected === fullPath;
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

  const handleOpenInternalSarcFile = () => {
    closeContextMenu();
    editInternalSarcFile(fullPath, setStatusText, setActiveTab, setLabelTextDisplay, updateEditorContent);
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
    backgroundColor: isSelected && isFile
      ? 'darkgray' // Darker background for selected nodeComponent/AnimationParamComponent/AnimationParam/Upper_Common.engine__component__AnimationParam.bgyml
      : sarcPaths.added_paths.includes(fullPath)
        ? '#205F63'
        : sarcPaths.modded_paths.includes(fullPath)
          ? '#826C00'
          : 'transparent'
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

    // If the tree container itself has a scroll, add this offset too
    // You need to replace '.tree-container' with the actual selector of your container
    const treeContainer = document.querySelector('.directory-tree');
    if (treeContainer) {
      offsetX += treeContainer.scrollLeft - treeContainer.getBoundingClientRect().left;
      offsetY += treeContainer.scrollTop - treeContainer.getBoundingClientRect().top;
    }

    setContextMenu({
      visible: true,
      x: e.clientX + offsetX,
      y: e.clientY + offsetY,
    });
    e.stopPropagation();
    onContextMenu && onContextMenu(fullPath);
  };

  const closeContextMenu = () => {
    setContextMenu({ visible: false, x: 0, y: 0 });
  };

  const contextMenuActions = isFile ? [
    // { label: 'Edit', method: () => console.log(`Edit clicked on ${fullPath}`) },
    { label: 'Edit', method: handleOpenInternalSarcFile },
    // { label: 'Replace', method: () => console.log(`Replace clicked on ${fullPath}`) },
    { label: 'Replace', method: handleReplaceInternalSarcFile },
    { label: 'Remove', method: handleRemoveInternalSarcFile },
    { label: 'Rename', method: handleRenameInternalSarcFile },
    { label: 'Copy path', method: () => handlePathToClipboard(fullPath) },
    { label: 'Copy name', method: () => handlePathToClipboard(name) },
    { label: 'Close', method: () => closeContextMenu() },
  ] : [
    { label: 'Add', method: handleAddInternalSarcFileToDir },
    { label: 'Remove', method: handleRemoveInternalSarcFile },
    { label: 'Rename', method: handleRenameInternalSarcFile },
    { label: 'Close', method: () => closeContextMenu() },
  ];

  return (
    <li onContextMenu={onContextMenu} onClick={handleSelect}>
      <div style={nodeStyle}
        onContextMenu={handleIconContextMenu}>
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
        />
      )}
    </li>
  );
};


export default DirectoryNode;
