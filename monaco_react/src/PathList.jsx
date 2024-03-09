import React, { useState } from 'react';

const fontsize = '15px';
const dirOpened = `dir_opened.png`;
const dirClosed = `dir_closed.png`;
const fileIcon = `file.png`;
const iconSize = '20px'; // Define the constant variable




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


// Helper function to build the tree 
const buildTree = (paths) => {
  //console.log(paths);
  const root = {};
  paths.paths.forEach((path) => {
    path.split('/').reduce((acc, name, index, arr) => {
      if (!acc[name]) {
        acc[name] = index === arr.length - 1 ? null : {}; // Null for files, {} for directories
      }
      return acc[name] || {};
    }, root);
  });
  return root;
};

// Recursive component to render nodes with smooth transitions
//const DirectoryNode = ({ node, name, path, onContextMenu, added_paths, modded_paths, selected,
const DirectoryNode = ({ node, name, path, onContextMenu, sarcPaths, selected,
  onSelect, }) => {
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [contextMenu, setContextMenu] = useState({ visible: false, x: 0, y: 0 });
  const isFile = node === null;
  const fullPath = path ? `${path}/${name}` : name;
  const endian = "LE";
  const isSelected = selected === fullPath;
  const handleSelect = (e) => {
    e.stopPropagation(); // This stops the event from bubbling up further
    console.log(`Selected: ${fullPath}`);
    onSelect(fullPath, endian); // Pass the fullPath to the onSelect function
  };



  const nodeStyle = {
    borderRadius: '5px',
    width: '95%',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    backgroundColor: isSelected && isFile
      ? 'darkgray' // Darker background for selected node
      : sarcPaths.added_paths.includes(fullPath)
        ? 'purple'
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
    { label: 'Edit', method: () => console.log(`Edit clicked on ${fullPath}`) },
    { label: 'Replace', method: () => console.log(`Replace clicked on ${fullPath}`) },
    { label: 'Remove', method: () => console.log(`Remove clicked on ${fullPath}`) },
    { label: 'Rename', method: () => console.log(`Rename clicked on ${fullPath}`) },
    { label: 'Close', method: () => closeContextMenu() },
  ] : [
    { label: 'Add', method: () => console.log(`Add clicked on ${fullPath}`) },
    { label: 'Remove', method: () => console.log(`Remove clicked on ${fullPath}`) },
    { label: 'Rename', method: () => console.log(`Rename clicked on ${fullPath}`) },
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

// Main DirectoryTree component (unchanged)
//const DirectoryTree = ({ paths, added_paths, modded_paths }) => {
const DirectoryTree = ({ onNodeSelect, sarcPaths }) => {
  const [selectedNode, setSelectedNode] = useState("");
  const tree = buildTree(sarcPaths);
  const handleSelectNode = (fullPath) => {
    setSelectedNode(fullPath, "LE");
    onNodeSelect(fullPath, "LE"); // Directly using destructured prop
  };


  const handleContextMenu = (fullPath) => {
    // alert(`Context menu for ${fullPath}`);
    setSelectedNode(fullPath);
  };


  return (
    <ul className="directory-tree" style={{ listStyleType: 'none', fontSize: fontsize }}>
      {Object.entries(tree).map(([key, value]) => (
        <DirectoryNode
          key={key}
          node={value}
          name={key}
          path=""
          onContextMenu={handleContextMenu}
          sarcPaths={sarcPaths}
          selected={selectedNode}
          onSelect={handleSelectNode}
        />
      ))}
    </ul>
  );
};

export default DirectoryTree;
