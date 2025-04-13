from pathlib import Path
import re
from dataclasses import asdict, dataclass, field
from typing import List, Optional


@dataclass
class Field:
    name: str
    type: str


@dataclass
class StructInfo:
    name: str
    inherited: Optional[str]
    fields: List[Field] = field(default_factory=list)
    size: Optional[int] = None


def parse_structs(header: str) -> List[StructInfo]:
    # Pattern for structs with inheritance
    struct_pattern = re.compile(
        r'struct\s+(?P<name>\w+)\s*:\s*public\s+(?P<base>\w+)\s*\{(?P<body>.*?)\};',
        re.DOTALL
    )

    # Pattern for structs without inheritance
    simple_struct_pattern = re.compile(
        r'struct\s+(?P<name>\w+)\s*\{(?P<body>.*?)\};',
        re.DOTALL
    )

    # Pattern for field declarations
    field_pattern = re.compile(r'\s*(?P<type>[\w:<>\*\s,]+?)\s+(?P<name>\w+);')

    # Pattern for static_assert(sizeof(...)) to get struct size
    size_pattern = re.compile(r'static_assert\(sizeof\((\w+)\)\s*==\s*0x([0-9A-Fa-f]+)L\);')

    structs = []

    # Parse structs with inheritance
    for match in struct_pattern.finditer(header):
        name, base, body = match.group("name"), match.group("base"), match.group("body")
        fields = [Field(name=m.group("name"), type=m.group("type").strip())
                  for m in field_pattern.finditer(body)]
        structs.append(StructInfo(name=name, inherited=base, fields=fields))

    # Parse structs without inheritance
    for match in simple_struct_pattern.finditer(header):
        name, body = match.group("name"), match.group("body")
        if any(s.name == name for s in structs):
            continue
        fields = [Field(name=m.group("name"), type=m.group("type").strip())
                  for m in field_pattern.finditer(body)]
        structs.append(StructInfo(name=name, inherited=None, fields=fields))

    # Parse sizes from static_assert
    sizes = {m.group(1): int(m.group(2), 16) for m in size_pattern.finditer(header)}
    for struct in structs:
        if struct.name in sizes:
            struct.size = sizes[struct.name]

    return structs


# Example usage
if __name__ == "__main__":
    import json
    with open(r"tmp\hk_types_totk1.2.1.h", "r") as f:  # Replace with your header file path
        header_code = f.read()

    structs = parse_structs(header_code)
    result = {}
    for s in structs:
        tmp = asdict(s)
        tmp["fields"] = [asdict(f) for f in s.fields]
        result[s.name] = tmp
        # print(f"Struct: {s.name}")
        # print(f"  Inherits: {s.inherited}")
        # print(f"  Size: {hex(s.size) if s.size else 'Unknown'}")
        # print("  Fields:")
        # for f in s.fields:
        #     print(f"    - {f.name}: {f.type}")
        # print()
    _pyclasses = json.loads(Path(r"resources\py_classes.json").read_text())
    result = {k:v for k,v in result.items() if k not in _pyclasses and len(v["fields"]) > 0}
    print(len(list(structs)))
    print(len(list(result)))
    with open("tmp/structs.json", "w") as f:
        json.dump(result, f, indent=4, sort_keys=False)