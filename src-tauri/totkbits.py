import io
import os
import sys
import tempfile
from typing import Dict
import oead
import evfl
CWD = os.path.dirname(os.path.abspath(__file__))
os.chdir(CWD) # Set the current working directory to the script's directory
sys.path.append(os.path.join(CWD, "bin/ainb/ainb"))
sys.path.append(os.path.join(CWD, "bin/asb"))
sys.path.append(os.path.join(CWD, "bin/ptcl"))

try:
    import bin.ainb.ainb.ainb as ainb_lib
    # from bin.ainb.ainb.converter import ainb_to_json, json_to_ainb, ainb_to_yaml, yaml_to_ainb
except ImportError as e:
    sys.stdout.buffer.write(b"Error Import: 16 " + str(e).encode("utf-8"))
try:
    from bin.asb.asb import ASB
except ImportError as e:
    sys.stdout.buffer.write(b"Error Import: 20 " + str(e).encode("utf-8"))
try:
    from bin.ptcl.ptcl  import ptcl_binary_to_text_lib, ptcl_apply_edits_lib
except ImportError as e:
    sys.stdout.buffer.write(b"Error Import: 24 " + str(e).encode("utf-8"))
import json
try:
    import yaml
except ImportError:
    raise ImportError("DID YOU EVEN TRY TO READ THE INSTRUCTIONS BEFORE YOU DID THIS? GO BACK TO THE GITHUB README AND LEARN TO READ :P")

    
# def evfl_text_to_binary(encoding="utf-8"): # Converts input JSON file to ASB
#     try:
#         data = sys.stdin.buffer.read()
#         json_data = yaml.safe_load(data.decode(encoding))
#         flow = evfl.EventFlow()
#         flow.read_json(json_data)
        
      
#         sys.stdout.buffer.write(new_rawdata.encode(encoding) if isinstance(new_rawdata, str) else new_rawdata)
     
#     except Exception as e:
#         sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))

    
def ptcl_text_to_binary(encoding="utf-8"): # Converts input JSON file to ASB
    try:
        merged_data = sys.stdin.buffer.read()
        if b"%PTCL_JSON%" not in merged_data:
            sys.stdout.buffer.write(b"Error: %PTCL_JSON% not found in input data")
            return
        ptcl_data, json_data = merged_data.split(b"%PTCL_JSON%")
        new_rawdata = ptcl_apply_edits_lib(ptcl_data, json_data)
        sys.stdout.buffer.write(new_rawdata.encode(encoding) if isinstance(new_rawdata, str) else new_rawdata)
     
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))

def ptcl_binary_to_text() -> str: # Converts input PTCL file to JSON
    try:
        data = sys.stdin.buffer.read()
        text = ptcl_binary_to_text_lib(data)
        sys.stdout.buffer.write(text if isinstance(text, bytes) else text.encode("utf-8"))
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode("utf-8"))
        
def evfl_binary_to_text() -> str: # Converts input PTCL file to JSON
    #TODO: Implement this function
    try:
        data = sys.stdin.buffer.read()
        
        text = ptcl_binary_to_text_lib(data)
        sys.stdout.buffer.write(text if isinstance(text, bytes) else text.encode("utf-8"))
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode("utf-8"))


def asb_binary_to_text(): # Converts input ASB file to JSON
    try:
        separator = b"%ASB_SEPARATOR%"
        
        data = sys.stdin.buffer.read()
        if separator not in data:
            sys.stdout.buffer.write(b"Error: %ASB_SEPARATOR% not found in input data")
            return
        if not data.startswith(b"ASB"):
            sys.stdout.buffer.write(b"Error: invalid ASB magic")     
            return   
        asb_data, baev_data = data.split(separator)
        file = ASB.from_binary(asb_data)
        if baev_data:
            file.import_baev(baev_data)
        _dict = file.asdict()
        
        # asb = ASB(data, from_json=False)
        # text = yaml.dump(_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        text = json.dumps(_dict, indent=4, ensure_ascii=False).encode("utf-8")
        # text = yaml.dump(asb.output_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        sys.stdout.buffer.write(text.encode("utf-8") if isinstance(text, str) else text)
    except Exception as e:
        sys.stdout.buffer.write(b"Error binary_to_text: " + str(e).encode("utf-8"))

def asb_text_to_binary(encoding="utf-8"): # Converts input JSON file to ASB
    try:
        separator = b"%ASB_SEPARATOR%"
        # sys.stdout.buffer.write(b"Executing command: asb_text_to_binary in function body\n")
        data = sys.stdin.buffer.read()
        json_data = json.loads(data.decode(encoding))
        # json_data = yaml.safe_load(data.decode(encoding))
        
        asb = ASB.from_dict(json_data)
        asb_data, baev_data = asb.to_binary_stream()
        result = asb_data + separator + baev_data
        # cursor = io.BytesIO(bytearray())
        # asb.ToBytes(cursor)
        sys.stdout.buffer.write(result)
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))
    
def ainb_binary_to_text(): # Converts input AINB file to JSON
    try:
        data = sys.stdin.buffer.read()
        file = ainb_lib.AINB(data)
        text = yaml.dump(file.output_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        sys.stdout.buffer.write(text)
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode("utf-8"))

def ainb_text_to_binary(encoding="utf-8"): # Converts input JSON file to AINB
    try:
        data = sys.stdin.buffer.read()
        # json_data = json.loads(data.decode(encoding))
        json_data = yaml.safe_load(data.decode(encoding))
        file = ainb_lib.AINB(json_data, from_dict=True)
        
        cursor = io.BytesIO(bytearray())
        file.ToBytes(file, cursor)
        sys.stdout.buffer.write(cursor.getvalue())
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))

def byml_text_to_binary(encoding="utf-8"): # Converts input JSON file to AINB
    try:
        data = sys.stdin.buffer.read()
        str_var = data.decode(encoding)
        pio = oead.byml.from_text(str_var)
        rawdata = oead.byml.to_binary(pio, big_endian=False)
        sys.stdout.buffer.write(bytes(rawdata))
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))

if __name__ == "__main__":
    if len(sys.argv) > 1:
        commands = {
            "ainb_binary_to_text": ainb_binary_to_text,
            "ainb_text_to_binary": ainb_text_to_binary,
            "asb_binary_to_text": asb_binary_to_text,
            "asb_text_to_binary": asb_text_to_binary,  
            "byml_text_to_binary": byml_text_to_binary,
            "ptcl_binary_to_text": ptcl_binary_to_text,
            "ptcl_text_to_binary": ptcl_text_to_binary
        }

        # Execute the function based on the command line argument
        if sys.argv[1] in commands.keys():
            # sys.stdout.write(f"Executing command '{sys.argv[1]}'\n")
            commands[sys.argv[1]]()
        else:
            print(f"Command '{sys.argv[1]}' not recognized.")
    else:
        sys.stdout.write("Hello from python")