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
const DirectoryNode = ({ node, name, path, onContextMenu }) => {
  const [isCollapsed, setIsCollapsed] = useState(true);
  const toggleCollapse = () => setIsCollapsed(!isCollapsed);
  const isFile = node === null;
  const fullPath = path ? `${path}/${name}` : name;

  const handleIconClick = (e) => {
    if (!isFile) {
      toggleCollapse();
    }
    e.stopPropagation(); // Prevents the click from bubbling to the div
  };

  const handleIconContextMenu = (e) => {
    e.preventDefault();
    e.stopPropagation(); // Prevents the context menu from affecting other elements
    onContextMenu(fullPath);
  };

  return (
    <li>
      <div style={{ cursor: 'pointer', display: 'flex', alignItems: 'center' }}>
        <img 
          src={isFile ? fileIcon : folderIcon} 
          alt={name} 
          style={{ marginRight: '5px', width: '20px', height: '20px' }} 
          onClick={handleIconClick}
          onContextMenu={handleIconContextMenu}
        />
        <span onClick={toggleCollapse}>{name}</span>
      </div>
      {!isCollapsed && node && (
        <ul style={{ marginLeft: '-10px', listStyleType: 'none' }}>
          {Object.entries(node).map(([key, value]) => (
            <DirectoryNode 
              key={key} 
              node={value} 
              name={key} 
              path={fullPath} // Pass down the full path
              onContextMenu={onContextMenu} 
            />
          ))}
        </ul>
      )}
    </li>
  );
};

// Main DirectoryTree component
const DirectoryTree = ({ paths }) => {
  const tree = buildTree(paths);

  const handleContextMenu = (fullPath) => {
    alert(`Context menu for ${fullPath}`);
  };

  return (
    <ul className="directory-tree" style={{ listStyleType: 'none' }}>
      {Object.entries(tree).map(([key, value]) => (
        <DirectoryNode 
          key={key} 
          node={value} 
          name={key} 
          path="" // Start with an empty string for the root path
          onContextMenu={handleContextMenu} 
        />
      ))}
    </ul>
  );
};

export default DirectoryTree;
