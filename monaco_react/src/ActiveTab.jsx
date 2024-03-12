import React, { useEffect, useRef, useState } from 'react';

export const BackendEnum = {
  SARC: 'SARC',
  YAML: 'YAML',
};

function ActiveTabDisplay({ activeTab, setActiveTab, labelTextDisplay }) {
  const labelTextRef = useRef(null);
  const activetabRef = useRef(null);
  const [windowWidth, setWindowWidth] = useState(window.innerWidth);
  const [labelTextWidth, setLabelTextWidth] = useState(0);
  const [activetabWidth, setActivetabWidth] = useState(0);

  const switchTab = (option) => {
    setActiveTab(option);
  }

  useEffect(() => {
    const handleResize = () => {
      setWindowWidth(window.innerWidth);

      if (labelTextRef.current) {
        setLabelTextWidth(labelTextRef.current.getBoundingClientRect().width);
      }
      if (activetabRef.current) {
        setActivetabWidth(activetabRef.current.getBoundingClientRect().width);
      }
    };

    handleResize();
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []); // Removed activeTab and windowWidth from dependency array

  const label = (() => {
    switch (activeTab) {
      case BackendEnum.SARC:
        return labelTextDisplay.sarc;
      case BackendEnum.YAML:
        return labelTextDisplay.yaml;
      default:
        return '';
    }
  })();

  return (
    <div ref={activetabRef}>
      <div className="activetab">
        <label
          key={"SARC"}
          className={activeTab === "SARC" ? "active" : ""}
          onClick={() => switchTab("SARC")}
        >
          {"SARC"}
        </label>
        <label
          key={"YAML"}
          className={activeTab === "YAML" ? "active" : ""}
          onClick={() => switchTab("YAML")}
        >
          {"YAML"}
        </label>
        {
          windowWidth - labelTextWidth >= 140 && (
            <div className="activetablabel" ref={labelTextRef}>
              {label}
            </div>
          )
        }
      </div>
    </div>
  );
}

export default ActiveTabDisplay;
