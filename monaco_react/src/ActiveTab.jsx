// ActiveTabDisplay.js
import React, { useState, useEffect, useRef } from 'react';

export const BackendEnum = {
  SARC: 'SARC',
  YAML: 'YAML',
  Options: 'Options',
};

function ActiveTabDisplay({ activeTab, setActiveTab, labelTextDisplay, setLabelTextDisplay }) {
  const labelTextRef = useRef(null);
  const activetabRef = useRef(null);
  const [windowWidth, setWindowWidth] = useState(window.innerWidth);
  const [labelTextWidth, setLabelTextWidth] = useState(0);
  const [activetabWidth, setActivetabWidth] = useState(0);
  // const [labelTextDisplay, setLabelTextDisplay] = useState('');
  const [shouldShowLabel, setShouldShowLabel] = useState(true);

  const labelText = () => {
    switch (activeTab) {
      case BackendEnum.SARC:
        return 'SArc';
      case BackendEnum.YAML:
        return 'Armor_006_Upper.engine__actor__ActorParam.bgyml';
      case BackendEnum.Options:
        return '';
      default:
        return '';
    }
  }




  // Function to update the window width state
  const handleResize = () => {
    setWindowWidth(window.innerWidth);
  };
  const switchTab = (option) => {
    setActiveTab(option);
    setLabelTextDisplay(labelText());
  }

  // Use useEffect to add event listener on mount and cleanup on unmount
  useEffect(() => {

    const handleResize = () => {
      // Immediately update windowWidth state
      setWindowWidth(window.innerWidth);

      if (labelTextRef.current) {
        setLabelTextWidth(labelTextRef.current.getBoundingClientRect().width);
      }
      if (activetabRef.current) {
        setActivetabWidth(activetabRef.current.getBoundingClientRect().width);
      }
    };
    // Measure immediately and then add event listener for future resizes
    setLabelTextDisplay(labelText());
    handleResize();
    window.addEventListener('resize', handleResize);
    // Cleanup the event listener on component unmount
    return () => {
      window.removeEventListener('resize', handleResize);
    };
  }, [activeTab, labelTextDisplay, windowWidth]); // Recalculate when activeTab changes


  return (
    <div ref={activetabRef}>
      <div className="activetab" >
        {Object.values(BackendEnum).map((option) => (
          <label
            key={option}
            className={activeTab === option ? "active" : ""}
            onClick={() => switchTab(option)}
          >
            {option}
          </label>
        ))}
          {
            windowWidth - labelTextWidth >= 300 && (
              <div className="activetablabel" ref={labelTextRef}>
                {/* Your label text here */}
                {labelTextDisplay}
              </div>
            )
          }
      </div>
      {/* Other menu items */}
    </div>
  );
}

export default ActiveTabDisplay;
