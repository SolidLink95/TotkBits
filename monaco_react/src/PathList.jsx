import React, { useState, useEffect } from 'react';

const folderIcon = `dir_opened.png`;
const fileIcon = `file.png`;


// Helper function to build the tree
const buildTree = (paths) => {
  const root = {};
  paths.forEach((path) => {
    path.split('/').reduce((acc, name, index, arr) => {
      if (!acc[name]) {
        acc[name] = index === arr.length - 1 ? null : {}; // Null for files, {} for directories
      }
      return acc[name] || {};
    }, root);
  });
  return root;
};

// Recursive component to render nodes
const DirectoryNode = ({ node, name, onContextMenu }) => {
  const [isCollapsed, setIsCollapsed] = useState(true);
  const isFile = node === null;

  // Toggle the collapsed state for folders
  const toggleCollapse = () => {
    if (!isFile) {
      setIsCollapsed(!isCollapsed);
    }
  };

  return (
    <li>
      <div onClick={toggleCollapse} style={{ cursor: 'pointer', display: 'flex', alignItems: 'center' }}>
        <img 
          src={isFile ? fileIcon : folderIcon} 
          alt="" 
          style={{ marginRight: '5px',width: '20px', height: '20px' }} 
          onClick={(e) => e.stopPropagation()} // Prevents the click from bubbling to the div
          onContextMenu={(e) => onContextMenu(e, name)} // Attaches the context menu to the icon
        />
        {name}
      </div>
      {!isCollapsed && node && (
        <ul style={{ marginLeft: '-10px', listStyleType: 'none' }}>
          {Object.entries(node).map(([key, value]) => (
            <DirectoryNode key={key} node={value} name={key} onContextMenu={onContextMenu} />
          ))}
        </ul>
      )}
    </li>
  );
};
// Main DirectoryTree component
const DirectoryTree = ({ paths }) => {
  const tree = buildTree(paths);

  // Context menu handler
  const handleContextMenu = (event, fileName) => {
    event.preventDefault();
    alert(`Context menu for ${fileName}`);
    // Here, implement your logic to show a custom context menu
  };

  return (
    <ul className="directory-tree">
      {Object.entries(tree).map(([key, value]) => (
        <DirectoryNode key={key} node={value} name={key} onContextMenu={handleContextMenu} />
      ))}
    </ul>
  );
};

export default DirectoryTree;
