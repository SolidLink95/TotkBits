import React from 'react';

// A mock list of paths as an example
const paths = [
  "Actor/Armor_006_Upper_engine_actor_ActorParam.byml",
  "Component/ActorPositionCalculatorParam/AnimationParam/ArmorParam/Armor_006_Upper_game_component_ArmorParam.byml",
  // ... more paths
];

function PathList() {
  return (
    <div className='pathlist'>
      {paths.map((path, index) => (
        <div key={index} style={{ paddingLeft: `${path.split('/').length * 10}px` }}>
          {path.split('/').pop()}
        </div>
      ))}
    </div>
  );
}

export default PathList;
