// ActiveTabDisplay.js
import React, { useState } from 'react';

export const BackendEnum = {
  SARC: 'SARC',
  YAML: 'YAML',
  Options: 'Options',
};

function ActiveTabDisplay({ activeTab, setActiveTab }) {
  return (
    <div className="activetab">
      {Object.values(BackendEnum).map((option) => (
        <label
          key={option}
          className={activeTab === option ? "active" : ""}
          onClick={() => setActiveTab(option)}
        >
          {option}
        </label>
      ))}
    </div>
  );
}

export default ActiveTabDisplay;
