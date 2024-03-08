import React, { useEffect, useRef, useState } from "react";
import "./App.css";
import { useExitApp } from "./ButtonClicks";

function MenuBarDisplay() {
  const [showDropdown, setShowDropdown] = useState({ file: false, view: false, tools: false });
  const dropdownRefs = useRef({ file: null, view: null, tools: null });

  const toggleDropdown = (menu) => {
    setShowDropdown(prevState => ({
      ...{ file: false, view: false, tools: false }, // Reset all to false
      [menu]: !prevState[menu] // Then toggle the clicked one
    }));
  };


  useEffect(() => {
    function reset() { 
      setShowDropdown({ file: false, view: false, tools: false });
    }
    function handleClickOutside(event) {
      // Get an array of all dropdown DOM nodes
      const dropdownNodes = Object.values(dropdownRefs.current).filter(Boolean);
      // Check if the click target is not contained within any dropdown node
      const isOutside = dropdownNodes.every(node => !node.contains(event.target));
      if (isOutside) {
        setShowDropdown({ file: false, view: false, tools: false });
      }
    }

    // Add click event listener
    document.addEventListener('mousedown', handleClickOutside);

    // Cleanup event listener
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, []);

  return (
    <div className="menu-bar">
      <div className="menu-item" onClick={() => toggleDropdown('file')} ref={el => dropdownRefs.current.file = el}>
        File
        <div className="dropdown-content" style={{ display: showDropdown.file ? 'block' : 'none' }}>
          <a href="#">Open</a>
          <a href="#">Save</a>
          <a href="#">Save as</a>
          <a href="#">Close all</a>
          <a href="#" onClick={useExitApp}>Exit</a >
        </div>
      </div>
      <div className="menu-item" onClick={() => toggleDropdown('tools')} ref={el => dropdownRefs.current.tools = el}>
        Tools
        <div className="dropdown-content" style={{ display: showDropdown.tools ? 'block' : 'none' }}>
          <a href="#">Edit</a>
          <a href="#">Extract</a>
          <a href="#">Find</a>
          <a href="#">Settings</a>
        </div>
      </div>
    </div>
  );
}

export default MenuBarDisplay;
