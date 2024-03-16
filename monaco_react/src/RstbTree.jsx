import React, { useState } from 'react';
import DirectoryNode from './DirectoryNode';
import { extractFileClick, editInternalSarcFile, fetchAndSetEditorContent, saveAsFileClick, saveFileClick } from './ButtonClicks';

const fontsize = '15px';

//{ editorRef, updateEditorContent, setStatusText, activeTab, setActiveTab, setLabelTextDisplay, setpaths, selectedPath, changeModal }
const RstbTree = ({ onNodeSelect, sarcPaths , setStatusText, activeTab}) => {
  const [selectedNode, setSelectedNode] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const data = [
    {path: "x", val: "1"},
    {path: "a", val: "2"},
    {path: "b", val: "3"},
    {path: "c", val: "4"},
  ];


  // if (activeTab !== 'SARC') { 
  //   return null;
  // }
  //style={{  display: activeTab === 'SARC' ? "block" : "none" }}
  return (
    <>
      {/* <ul className="directory-tree" 
        style={{ listStyleType: 'none', marginBottom: '88px', //width: activeTab === 'SARC' ? "100%" : "0%", 
        ...activeTab !== 'RSTB' ? { height: '0%', width: '0%' } : {}
         }}//robust solution to hide tree when not active. This way collapsed nodes states are not lost
      > */}
        {/* {Object.entries(renderTree).map(([key, value]) => (
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
        ))} */}
      {/* </ul> */}
      {activeTab === 'RSTB' && <div className='textsearch' style={{ padding: '10px' }}>
        <input
          type="text"
          placeholder="Search..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{ width: '100%', padding: '5px' }}
        />
        <button onClick={() => setSearchQuery("")} style={{ width: '70px', padding: '5px' }}>Clear</button>
      </div>}
    </>
  );
};

export default RstbTree;
