import React, { useState } from 'react';
import DirectoryNode from './DirectoryNode'; 

const fontsize = '15px';

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
