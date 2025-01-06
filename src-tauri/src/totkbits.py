import io
import os
import sys
import oead
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
    # asdf()
    # sys.exit()
    if len(sys.argv) > 1:
        # sys.stdout.buffer.write(b"Executing command: " + sys.argv[1].encode("utf-8") + b"\n")
        commands = {
            "ainb_binary_to_text": ainb_binary_to_text,
            "ainb_text_to_binary": ainb_text_to_binary,
            "asb_binary_to_text": asb_binary_to_text,
            "asb_text_to_binary": asb_text_to_binary,  
            "byml_text_to_binary": byml_text_to_binary
        }

        # Execute the function based on the command line argument
        if sys.argv[1] in commands.keys():
            # sys.stdout.write(f"Executing command '{sys.argv[1]}'\n")
            commands[sys.argv[1]]()
        else:
            print(f"Command '{sys.argv[1]}' not recognized.")
    else:
        sys.stdout.write("Hello from python")