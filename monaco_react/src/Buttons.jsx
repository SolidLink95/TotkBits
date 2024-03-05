import React from 'react';

  function ButtonsDisplay() {


    function ImageButton({ src, onClick, alt }) {
        // Apply both the background image and styles directly to the button
        return (
          <button
            onClick={onClick}
            className='button'
            style={{
              backgroundImage: `url(${src})`,
              backgroundSize: 'cover', // Cover the entire area of the button
              backgroundPosition: 'center', // Center the background image
              width: '40px', // Define your desired width
              height: '40px', // Define your desired height
              display: 'flex', // Ensure the button content (if any) is centered
              justifyContent: 'left', // Center horizontally
              alignItems: 'left', // Center vertically
            }}
            aria-label={alt} // Accessibility label for the button if the image fails to load or for screen readers
          >
            {/* You can still add text or icons inside the button if necessary */}
          </button>
        );
      }
      const imageButtonsData = [
          { src: 'open.png', alt: 'Description of Image 1', onClick: () => console.log('Button 1 clicked') },
          { src: 'path/to/image2.png', alt: 'Description of Image 2', onClick: () => console.log('Button 2 clicked') },
          { src: 'path/to/image3.png', alt: 'Description of Image 3', onClick: () => console.log('Button 3 clicked') },
          { src: 'path/to/image4.png', alt: 'Description of Image 4', onClick: () => console.log('Button 4 clicked') },
          { src: 'path/to/image5.png', alt: 'Description of Image 5', onClick: () => console.log('Button 5 clicked') },
        ];
      

    return (
      <div className="buttons-container">
        {imageButtonsData.map((button, index) => (
          <ImageButton key={index} src={button.src} alt={button.alt} onClick={button.onClick} />
        ))}
      </div>
    );
  }

export default ButtonsDisplay;