import re
from dataclasses import asdict, dataclass, field
from typing import List, Optional


@dataclass
class PyField:
    name: str
    type: str


@dataclass
class PyClassInfo:
    name: str
    base: Optional[str]
    fields: List[PyField] = field(default_factory=list)


def parse_python_classes(source: str) -> List[PyClassInfo]:
    # Match dataclass definitions
    class_pattern = re.compile(
        r'@dataclass\s*\nclass\s+(\w+)\(([\w\.]+)?\):\n(.*?)(?=\n@|^class\s|\Z)', re.DOTALL | re.MULTILINE
    )
    # Match field definitions (optionally with comments)
    field_pattern = re.compile(r'^\s*(\w+):\s*([\w\[\]#<>\. ]+)', re.MULTILINE)

    classes = []

    for class_match in class_pattern.finditer(source):
        name = class_match.group(1)
        base = class_match.group(2)
        body = class_match.group(3)

        fields = []
        for field_match in field_pattern.finditer(body):
            field_name = field_match.group(1)
            field_type = field_match.group(2).strip()
            fields.append(PyField(name=field_name, type=field_type))

        classes.append(PyClassInfo(name=name, base=base, fields=fields))

    return classes


# Example usage
if __name__ == "__main__":
    # Replace this with reading from a file if needed
    result = {}
    for file in ["Experimental.py", "BphclMainDataclasses.py", "BphclSmallDataclasses.py", "Havok.py"]:
        with open(file, "r") as f:
            source_code = f.read()

        parsed_classes = parse_python_classes(source_code)
        
        for cls in parsed_classes:
            tmp = asdict(cls)
            tmp["fields"] = [asdict(f) for f in cls.fields]
            result[cls.name] = tmp
            # print(f"Class: {cls.name}")
            # print(f"  Base: {cls.base or 'None'}")
            # print(f"  Fields:")
            # for field in cls.fields:
            #     print(f"    - {field.name}: {field.type}")
            # print()
    import json
    with open("tmp/py_classes.json", "w") as f:
        json.dump(result, f, indent=4)
