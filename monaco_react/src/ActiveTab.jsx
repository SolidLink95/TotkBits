// ActiveTabDisplay.js
import React, { useEffect, useRef, useState } from 'react';

export const BackendEnum = {
  SARC: 'SARC',
  YAML: 'YAML',
 // RSTB: 'RSTB',
};

function ActiveTabDisplay({ activeTab, setActiveTab, labelTextDisplay }) {
  const labelTextRef = useRef(null);
  const activetabRef = useRef(null);
  const [windowWidth, setWindowWidth] = useState(window.innerWidth);
  const [labelTextWidth, setLabelTextWidth] = useState(0);
  const [activetabWidth, setActivetabWidth] = useState(0);
  // const [labelTextDisplay, setLabelTextDisplay] = useState('');
  const [shouldShowLabel, setShouldShowLabel] = useState(true);





  // Function to update the window width state
  const handleResize = () => {
    setWindowWidth(window.innerWidth);
  };
  const switchTab = (option) => {
    setActiveTab(option);
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
    handleResize();
    window.addEventListener('resize', handleResize);
    // Cleanup the event listener on component unmount
    return () => {
      window.removeEventListener('resize', handleResize);
    };
  }, [activeTab, labelTextDisplay, windowWidth]); // Recalculate when activeTab changes

  const label = (() => {
    switch (activeTab) {
      case BackendEnum.SARC:
        return labelTextDisplay.sarc;
      case BackendEnum.YAML:
        return labelTextDisplay.yaml;
      default:
        return ''; // Return an empty string for any other case
    }
  })();

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
          windowWidth - labelTextWidth >= 140 && (
            <div className="activetablabel" ref={labelTextRef}>
              {/* Your label text here */}
              {label}
            </div>
          )
        }
      </div>
      {/* Other menu items */}
    </div>
  );
}

export default ActiveTabDisplay;
