import io
import  ainb
from converter import ainb_to_json, json_to_ainb, ainb_to_yaml, yaml_to_ainb

import json
try:
    import yaml
except ImportError:
    raise ImportError("DID YOU EVEN TRY TO READ THE INSTRUCTIONS BEFORE YOU DID THIS? GO BACK TO THE GITHUB README AND LEARN TO READ :P")
import sys
import os
import zlib



def binary_to_text(): # Converts input AINB file to JSON
    try:
        data = sys.stdin.buffer.read()
        file = ainb.AINB(data)
        # text = json.dumps(file.output_dict, ensure_ascii=False, indent=4)
        text = yaml.dump(file.output_dict, sort_keys=False, allow_unicode=True, indent=4, encoding='utf-8')
        # print(text)
        sys.stdout.buffer.write(text)
    except Exception as e:
        sys.stdout.buffer.write(b"Error: " + str(e).encode("utf-8"))

def text_to_binary(encoding="utf-8"): # Converts input JSON file to AINB
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
    if len(sys.argv) > 1:
        if sys.argv[1] == "binary_to_text":
            binary_to_text()
        elif sys.argv[1] == "text_to_binary":
            text_to_binary()
    else:
        sys.stdout.write("Hello from python")