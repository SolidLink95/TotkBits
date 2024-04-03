import io
import os
import sys
# sys.stdout.buffer.write(f"TEST asdf\n")
sys.path.append(os.path.join(os.getcwd(), "bin/ainb/ainb"))
sys.path.append(os.path.join(os.getcwd(), "bin/asb"))
sys.path.append(os.path.join(os.getcwd(), "src-tauri/bin/ainb/ainb"))
sys.path.append(os.path.join(os.getcwd(), "src-tauri/bin/asb"))
# from asb import ASB, asb_from_zs
# sys.stdout.write(f"{os.getcwd()}\n")
try:
    import ainb
    from converter import ainb_to_json, json_to_ainb, ainb_to_yaml, yaml_to_ainb
except ImportError as e:
    sys.stderr.buffer.write(b"Error Import: " + str(e).encode("utf-8"))
try:
    from asb import ASB, asb_from_zs
except ImportError as e:
    sys.stderr.buffer.write(b"Error Import: " + str(e).encode("utf-8"))
import json
try:
    import yaml
except ImportError:
    raise ImportError("DID YOU EVEN TRY TO READ THE INSTRUCTIONS BEFORE YOU DID THIS? GO BACK TO THE GITHUB README AND LEARN TO READ :P")

# Modify asb.py to include the following functions
# class ASB:
#     def __init__(self, data,from_json = False): #binary_to_text: False, text_to_binary: True
#         # from_json = False

    # # def ToBytes(self, cursor, output_dir=''):
    # def ToBytes(self, cursor):
    #     # if output_dir:
    #     #     os.makedirs(output_dir, exist_ok=True)
    #     # with open(os.path.join(output_dir, self.filename + ".asb"), 'wb') as f:
    #     with cursor as f:
# add "return f" at the end of the function

def asb_binary_to_text(): # Converts input ASB file to JSON
    try:
        data = sys.stdin.buffer.read()
        if not data.startswith(b"ASB"):
            sys.stdout.buffer.write(b"Error: invalid ASB magic")     
            return   
        asb = ASB(data, from_json=False)
        text = json.dumps(asb.output_dict, ensure_ascii=False, indent=4)
        # text = yaml.dump(asb.output_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        sys.stdout.buffer.write(text.encode("utf-8"))
    except Exception as e:
        sys.stdout.buffer.write(b"Error binary_to_text: " + str(e).encode("utf-8"))

def asb_text_to_binary(encoding="utf-8"): # Converts input JSON file to ASB
    try:
        # sys.stdout.buffer.write(b"Executing command: asb_text_to_binary in function body\n")
        data = sys.stdin.buffer.read()
        json_data = json.loads(data.decode(encoding))
        # json_data = yaml.safe_load(data.decode(encoding))
        
        asb = ASB(json_data, from_json=True)
        
        cursor = io.BytesIO(bytearray())
        asb.ToBytes(cursor)
        sys.stdout.buffer.write(cursor.getvalue())
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))

def asdf():
    x = """{
    "Info": {
        "Magic": "ASB ",
        "Version": "0x417",
        "Filename": "Accessory_Battery.root"
    },
    "Local Blackboard Parameters": {
        "float": [
            {
                "Name": "IncreasePartFrame",
                "Init Value": 0.0
            }
        ]
    },
    "Commands": [
        {
            "Name": "Amount",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "beb5eb31-621f-4288-d29d-2b4aada44c68",
            "Left Node Index": 4,
            "Right Node Index": -1
        },
        {
            "Name": "Amount_Run",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "4b372900-9c7d-48a5-7fbd-03d590b58f4f",
            "Left Node Index": 21,
            "Right Node Index": -1
        },
        {
            "Name": "Capacity",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "2e88f893-6b23-4546-f78a-003630c9b7f5",
            "Left Node Index": 1,
            "Right Node Index": -1
        },
        {
            "Name": "Extra",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "96308af4-bbad-4bf9-678e-d340fa78650c",
            "Left Node Index": 13,
            "Right Node Index": -1
        },
        {
            "Name": "Flash",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "b3f1b9f5-c605-425f-77ba-668dff218997",
            "Left Node Index": 10,
            "Right Node Index": -1
        },
        {
            "Name": "Heal",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "2e5fb645-b4ee-49d5-9886-cb911ed9b8ea",
            "Left Node Index": 12,
            "Right Node Index": -1
        },
        {
            "Name": "Increase",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "5fff0179-5ac0-412e-69b1-833ea256d332",
            "Left Node Index": 17,
            "Right Node Index": -1
        },
        {
            "Name": "IncreaseSt",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "64bdbe91-4f45-432e-48a8-73cbb647891f",
            "Left Node Index": 23,
            "Right Node Index": -1
        },
        {
            "Name": "Infinity",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "b12d2628-7bdc-441e-12b4-4caa3f39ef97",
            "Left Node Index": 14,
            "Right Node Index": -1
        },
        {
            "Name": "None",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "87529506-a5f0-46af-d0ae-bf1a85014a74",
            "Left Node Index": 16,
            "Right Node Index": -1
        },
        {
            "Name": "Normal",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "983f5eaa-dd69-4e41-1b7-58496c9ae77d",
            "Left Node Index": 6,
            "Right Node Index": -1
        },
        {
            "Name": "Run",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "cab762bf-96c0-45c4-8587-685a01f701ab",
            "Left Node Index": 8,
            "Right Node Index": -1
        },
        {
            "Name": "Weak",
            "Unknown 1": -1.0,
            "Unknown 2": 0,
            "Unknown 3": 0,
            "GUID": "90345bd0-c0f9-4315-9494-6f5b9f3c1c2f",
            "Left Node Index": 11,
            "Right Node Index": -1
        }
    ],
    "Transitions": [],
    "Valid Tag List": [],
    "Animation Slots": [
        {
            "Unknown": 0,
            "Partial 1": "Capacity",
            "Partial 2": "",
            "Entries": [
                {
                    "Bone": "Root",
                    "Unknown 1": 0,
                    "Unknown 2": 1
                }
            ]
        },
        {
            "Unknown": 0,
            "Partial 1": "Amount",
            "Partial 2": "",
            "Entries": [
                {
                    "Bone": "Root",
                    "Unknown 1": 1,
                    "Unknown 2": 1
                }
            ]
        },
        {
            "Unknown": 0,
            "Partial 1": "Main",
            "Partial 2": "",
            "Entries": [
                {
                    "Bone": "Root",
                    "Unknown 1": 1,
                    "Unknown 2": 1
                }
            ]
        },
        {
            "Unknown": 0,
            "Partial 1": "Event",
            "Partial 2": "",
            "Entries": [
                {
                    "Bone": "Root",
                    "Unknown 1": 1,
                    "Unknown 2": 1
                }
            ]
        }
    ],
    "0x68 Section": [],
    "Nodes": {
        "0": {
            "Node Type": "SkeletalAnimation",
            "Unknown": 1,
            "GUID": "c4bf2a73-d294-4330-3696-cd7e54b98e52",
            "0x38 Entries": [
                {
                    "Type": 0,
                    "GUID": "eb95bff1-58b7-4504-b4a0-53e0b1b8eddd",
                    "Entry": {
                        "Start Frame": 0.0,
                        "Unknown 2": 0
                    }
                }
            ],
            "0x40 Entries": [],
            "Body": {
                "Animation": "Capacity",
                "Unknown 1": 0,
                "Unknown 2": 0,
                "Unknown 3": false,
                "Unknown 4": 0.0,
                "Frame Node Connections": [
                    2
                ]
            }
        },
        "1": {
            "Node Type": "Simultaneous",
            "Unknown": 1,
            "GUID": "839f48d-6222-4612-4e8d-e71fd3c7fcce",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown": 0,
                "Child Nodes": [
                    0,
                    3
                ]
            }
        },
        "2": {
            "Node Type": "FrameController",
            "Unknown": 1,
            "GUID": "a93989e3-77d6-4b6c-8c8c-e43fa70c1c18",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Animation Rate": 0.0,
                "Start Frame": 0.0,
                "End Frame": -1.0,
                "Unknown Flag": 0,
                "Loop Cancel Flag": false,
                "Unknown 2": false,
                "Unknown 3": -1,
                "Unknown 4": -1,
                "Unknown 5": false,
                "Unknown 6": -1.0,
                "Unknown 7": -1.0,
                "Unknown 8": -1.0,
                "Unknown 9": false,
                "Unknown 10": -1.0,
                "Unknown 11": false,
                "Unknown 12": 0,
                "Unknown 13": 0
            }
        },
        "3": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "fb489e8b-81ea-4cac-57b6-7c3a42077886",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Capacity",
                "Unknown 2": true,
                "Frame Node Connections": [
                    2
                ]
            }
        },
        "4": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "cc05702-78c5-4357-639b-f3aeb7a821e0",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Amount",
                "Unknown 2": true,
                "Frame Node Connections": [
                    5
                ]
            }
        },
        "5": {
            "Node Type": "FrameController",
            "Unknown": 1,
            "GUID": "52eeaa12-2bb1-45dd-1c9d-34e0a69fb8f1",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Animation Rate": 0.0,
                "Start Frame": 0.0,
                "End Frame": -1.0,
                "Unknown Flag": 0,
                "Loop Cancel Flag": false,
                "Unknown 2": false,
                "Unknown 3": -1,
                "Unknown 4": -1,
                "Unknown 5": false,
                "Unknown 6": -1.0,
                "Unknown 7": -1.0,
                "Unknown 8": -1.0,
                "Unknown 9": false,
                "Unknown 10": -1.0,
                "Unknown 11": false,
                "Unknown 12": 0,
                "Unknown 13": 0
            }
        },
        "6": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "a54a0f8a-fd0d-4f4b-5798-d0799a08cd56",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Normal",
                "Unknown 2": true
            }
        },
        "7": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "c1a66824-e508-4159-5faa-b16ffa54a5ac",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Flash",
                "Unknown 2": true
            }
        },
        "8": {
            "Node Type": "Sequential",
            "Unknown": 1,
            "GUID": "4dc2bf6b-8bb0-4794-e382-c3f1463110d5",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": false,
                "Unknown 2": 0,
                "Unknown 3": 0,
                "Child Nodes": [
                    7,
                    9
                ]
            }
        },
        "9": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "cd4e2181-ae43-4a1c-2b8d-7a8c0fbcd07f",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Run",
                "Unknown 2": true
            }
        },
        "10": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "32dc0551-a9f2-4404-74ba-062f0a85f20f",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Flash",
                "Unknown 2": true
            }
        },
        "11": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "8f2be6ba-ae6e-4d20-f2a8-743265630b15",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Weak",
                "Unknown 2": true
            }
        },
        "12": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "33d0b7e0-d4df-4834-ee9b-eb60ee7a08aa",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Heal",
                "Unknown 2": true
            }
        },
        "13": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "523b0f5-258-499b-a099-09ed4c4883ad",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "EX",
                "Unknown 2": true
            }
        },
        "14": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "28443a11-8067-42bf-e488-578be0ab4561",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Infinite",
                "Unknown 2": true
            }
        },
        "15": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "97f34479-cb02-4d7a-5fbd-1a717b4af5c2",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Increase",
                "Unknown 2": true,
                "Frame Node Connections": [
                    20
                ]
            }
        },
        "16": {
            "Node Type": "DummyAnimation",
            "Unknown": 1,
            "GUID": "6fcc1cd8-9acb-40ef-59a5-d6e378688898",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Frame": 0.0,
                "Unknown": false
            }
        },
        "17": {
            "Node Type": "Simultaneous",
            "Unknown": 1,
            "GUID": "d9c61a8a-ce0f-4092-2e8d-02b11143b83e",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown": 1,
                "Child Nodes": [
                    15,
                    18
                ]
            }
        },
        "18": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "c0fee08e-d120-4c9d-5293-de5536de3080",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "IncreasePart",
                "Unknown 2": true,
                "Frame Node Connections": [
                    19
                ]
            }
        },
        "19": {
            "Node Type": "FrameController",
            "Unknown": 1,
            "GUID": "23ae64cc-5dbb-421e-84b7-5282eecbd49b",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Animation Rate": 0.0,
                "Start Frame": {
                    "Flags": "0x8000",
                    "Type": "float",
                    "Local Blackboard Index": 0
                },
                "End Frame": -1.0,
                "Unknown Flag": 2,
                "Loop Cancel Flag": false,
                "Unknown 2": false,
                "Unknown 3": -1,
                "Unknown 4": -1,
                "Unknown 5": false,
                "Unknown 6": -1.0,
                "Unknown 7": -1.0,
                "Unknown 8": -1.0,
                "Unknown 9": false,
                "Unknown 10": -1.0,
                "Unknown 11": false,
                "Unknown 12": 0,
                "Unknown 13": 0
            }
        },
        "20": {
            "Node Type": "FrameController",
            "Unknown": 1,
            "GUID": "23697e8b-2357-4896-f95-1c8ec18023dc",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Animation Rate": 1.0,
                "Start Frame": 0.0,
                "End Frame": -1.0,
                "Unknown Flag": 2,
                "Loop Cancel Flag": false,
                "Unknown 2": false,
                "Unknown 3": -1,
                "Unknown 4": -1,
                "Unknown 5": false,
                "Unknown 6": -1.0,
                "Unknown 7": -1.0,
                "Unknown 8": -1.0,
                "Unknown 9": false,
                "Unknown 10": -1.0,
                "Unknown 11": false,
                "Unknown 12": 0,
                "Unknown 13": 0
            }
        },
        "21": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "5165bbe5-fcff-4ecd-e688-adbd05049463",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "Amount_Run",
                "Unknown 2": true,
                "Frame Node Connections": [
                    5
                ]
            }
        },
        "22": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "c3ddbed3-363f-4e57-f386-1de0c248154e",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "IncreaseSt",
                "Unknown 2": true
            }
        },
        "23": {
            "Node Type": "Simultaneous",
            "Unknown": 1,
            "GUID": "259213ac-3827-4ceb-eba2-04d379482904",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown": 1,
                "Child Nodes": [
                    22,
                    24
                ]
            }
        },
        "24": {
            "Node Type": "MaterialAnimation",
            "Unknown": 1,
            "GUID": "7f693089-96b8-4d78-19b2-48bb307655fe",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Unknown 1": 0,
                "Animation": "IncreasePart",
                "Unknown 2": true,
                "Frame Node Connections": [
                    25
                ]
            }
        },
        "25": {
            "Node Type": "FrameController",
            "Unknown": 1,
            "GUID": "54eb6fb1-5e63-4ef2-5c9f-12e734980169",
            "0x38 Entries": [],
            "0x40 Entries": [],
            "Body": {
                "Animation Rate": 0.0,
                "Start Frame": {
                    "Flags": "0x8000",
                    "Type": "float",
                    "Local Blackboard Index": 0
                },
                "End Frame": -1.0,
                "Unknown Flag": 2,
                "Loop Cancel Flag": false,
                "Unknown 2": false,
                "Unknown 3": -1,
                "Unknown 4": -1,
                "Unknown 5": false,
                "Unknown 6": -1.0,
                "Unknown 7": -1.0,
                "Unknown 8": -1.0,
                "Unknown 9": false,
                "Unknown 10": -1.0,
                "Unknown 11": false,
                "Unknown 12": 0,
                "Unknown 13": 0
            }
        }
    }
}"""
    
    json_data = json.loads(x)
    asb = ASB(json_data, from_json=True)
        
    cursor = io.BytesIO(bytearray())
    asb.ToBytes(cursor)
    data = cursor.getvalue()
    # print(data[:5])
    sys.stdout.buffer.write(data)

def ainb_binary_to_text(): # Converts input AINB file to JSON
    try:
        data = sys.stdin.buffer.read()
        file = ainb.AINB(data)
        # text = json.dumps(file.output_dict, ensure_ascii=False, indent=4)
        text = yaml.dump(file.output_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        # print(text)
        sys.stdout.buffer.write(text)
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode("utf-8"))

def ainb_text_to_binary(encoding="utf-8"): # Converts input JSON file to AINB
    try:
        data = sys.stdin.buffer.read()
        # json_data = json.loads(data.decode(encoding))
        json_data = yaml.safe_load(data.decode(encoding))
        file = ainb.AINB(json_data, from_dict=True)
        
        cursor = io.BytesIO(bytearray())
        file.ToBytes(file, cursor)
        sys.stdout.buffer.write(cursor.getvalue())
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))

if __name__ == "__main__":
    # asdf()
    # sys.exit()
    if len(sys.argv) > 1:
        # sys.stdout.buffer.write(b"Executing command: " + sys.argv[1].encode("utf-8") + b"\n")
        commands = {
            "ainb_binary_to_text": ainb_binary_to_text,
            "ainb_text_to_binary": ainb_text_to_binary,
            "asb_binary_to_text": asb_binary_to_text,
            "asb_text_to_binary": asb_text_to_binary,  
            "asdf": asdf  
        }

        # Execute the function based on the command line argument
        if sys.argv[1] in commands.keys():
            # sys.stdout.write(f"Executing command '{sys.argv[1]}'\n")
            commands[sys.argv[1]]()
        else:
            print(f"Command '{sys.argv[1]}' not recognized.")
    else:
        sys.stdout.write("Hello from python")