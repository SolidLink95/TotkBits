import io
import os
import sys
import tempfile
from typing import Dict
import oead
import evfl
CWD = os.getcwd()
sys.path.append(os.path.join(CWD, "bin/ainb/ainb"))
sys.path.append(os.path.join(CWD, "bin/asb"))
sys.path.append(os.path.join(CWD, "bin/ptcl"))

try:
    import bin.ainb.ainb.ainb as ainb_lib
    # from bin.ainb.ainb.converter import ainb_to_json, json_to_ainb, ainb_to_yaml, yaml_to_ainb
except ImportError as e:
    sys.stdout.buffer.write(b"Error Import: 16 " + str(e).encode("utf-8"))
try:
    from bin.asb.asb import ASB, asb_from_zs
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
        flow = evfl.EventFlow()
        flow.read(data)
        res = {}
        # res['name'] = flow.name
        res["version"] = "0.3.0.0"
        res["Flowcharts"] = {}
        res["Timelines"] = {}
        res["Flowcharts"]["Actors"] = []
        res["Flowcharts"]["Events"] = []
        res["Flowcharts"]["EntryPoints"] = {}
        for a in flow.flowchart.actors:
            res["Flowcharts"]["Actors"].append({
                "Name": a.identifier.name,
                "SecondaryName": a.identifier.sub_name,
                "ArgumentName": a.argument_name,
                "EntryPointIndex": -1 if a.argument_entry_point is None else a.argument_entry_point,
                # "CutNumber" : getattr(a.params, 'concurrent_clips', -1),
                "CutNumber" : a.params.data.get('concurrent_clips', -1),
                "Actions": [x.v for x in a.actions],
                "Queries": [x.v for x in a.queries],
                "Params": dict(a.params.data)
            })
        for e in flow.flowchart.events:
            res["Flowcharts"]["Events"][e.name] = {
                
            }
        for t in flow.flowchart.timeline:
            res["Timelines"][t.name] = {
                "Name": t.name,
                "SecondaryName": t.identifier.sub_name,
                "ArgumentName": t.argument_name,
                "EntryPointIndex": -1 if t.argument_entry_point is None else t.argument_entry_point,
                # "CutNumber" : getattr(t.params, 'concurrent_clips', -1),
                "CutNumber" : t.params.data.get('concurrent_clips', -1),
                "Actions": [x.v for x in t.actions],
                "Queries": [x.v for x in t.queries],
                "Params": dict(t.params.data)
            }
        text = ptcl_binary_to_text_lib(data)
        sys.stdout.buffer.write(text if isinstance(text, bytes) else text.encode("utf-8"))
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode("utf-8"))


def asb_binary_to_text(): # Converts input ASB file to JSON
    try:
        data = sys.stdin.buffer.read()
        if not data.startswith(b"ASB"):
            sys.stdout.buffer.write(b"Error: invalid ASB magic")     
            return   
        asb = ASB(data, from_json=False)
        text = yaml.dump(asb.output_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        # text = yaml.dump(asb.output_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        sys.stdout.buffer.write(text.encode("utf-8") if isinstance(text, str) else text)
    except Exception as e:
        sys.stdout.buffer.write(b"Error binary_to_text: " + str(e).encode("utf-8"))

def asb_text_to_binary(encoding="utf-8"): # Converts input JSON file to ASB
    try:
        # sys.stdout.buffer.write(b"Executing command: asb_text_to_binary in function body\n")
        data = sys.stdin.buffer.read()
        # json_data = json.loads(data.decode(encoding))
        json_data = yaml.safe_load(data.decode(encoding))
        
        asb = ASB(json_data, from_json=True)
        
        cursor = io.BytesIO(bytearray())
        asb.ToBytes(cursor)
        sys.stdout.buffer.write(cursor.getvalue())
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode(encoding))
    
def ainb_binary_to_text(): # Converts input AINB file to JSON
    try:
        # import bin.ainb.ainb.ainb as ainb
        data = sys.stdin.buffer.read()
        file = ainb_lib.AINB(data)
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