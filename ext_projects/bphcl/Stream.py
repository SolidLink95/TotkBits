import io
import struct

from util import _hex, hexInt


class ReadStream(io.BytesIO):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.endian = "little"
        self.sign = "<" if self.endian == "little" else ">"
        self.max_str_size = 256
        self.encoding = "utf-8"
        self.data_offset = None # Placeholder for data offset
    
    def tell(self):
        return hexInt(super().tell())
    
    def get_data_size(self) -> int:
        current_pos = self.tell()
        self.seek_end()
        size = self.tell()
        self.seek(current_pos, io.SEEK_SET)  # Restore the original position
        return size
    
    def seek_end(self):
        """Seek to the end of the stream."""
        self.seek(0, io.SEEK_END)
        
    def seek_start(self):
        """Seek to the end of the stream."""
        self.seek(0, io.SEEK_SET)
    
    def peek(self, size: int) -> bytes:
        """Peek at the next `size` bytes without advancing the stream position."""
        current_pos = self.tell()
        data = self.read(size)
        self.seek(current_pos)
        return data
    
    def __repr__(self):
        # cname = self.__class__.__name__
        # return f"<{cname} position={self.tell()}>"
        cname = self.__class__.__name__
        # data_left = hexInt(self.get_data_size() - self.tell())
        is_def = False
        cur_pos = self.tell()
        prev_8_bytes = b""
        try:
            self.seek(-8, io.SEEK_CUR) # Move back to the original position
            prev_8_bytes = self.read(8).hex().upper() # Move back to the original position
            self.seek(cur_pos, io.SEEK_SET) # Move back to the original position
        except:
            pass
        try:
            cur_pos = self.tell()
            next_8_bytes = self.read(8).hex().upper() # Move back to the original position
            self.seek(cur_pos, io.SEEK_SET) # Move back to the original position
            _hex_bytes = ' '.join(next_8_bytes[i:i+2] for i in range(0, len(next_8_bytes), 2))
            _hex_bytes_prev = ' '.join(prev_8_bytes[i:i+2] for i in range(0, len(prev_8_bytes), 2))
            # 0x50 is DATA offset
            res =  f"<{cname} abspos={_hex(self.tell()+0x50)} pos={_hex(self.tell())} next={_hex_bytes} prev={_hex_bytes_prev} >"
            return res
        except:
            return f"<{cname} pos={_hex(self.tell())} >"
    
    def __str__(self):
        return f"<ReadStream position=0x{self.tell():08X}>"

    def align_to(self, alignment: int = 8):
        """Align the stream's position to the next multiple of `alignment`."""
        current_pos = self.tell()
        if current_pos == 0:
            return
        padding = (alignment - (current_pos % alignment)) % alignment
        if padding:
            self.seek(padding, io.SEEK_CUR)
    
    def _read_exact(self, size: int) -> bytes:
        data = self.read(size)
        if len(data) != size:
            raise EOFError(f"Expected {size} bytes, got {len(data)} bytes")
        return data

    def find_next_occ(self, data: bytes) -> int:
        current_pos = self.tell()
        found_pos = self.read().find(data)
        self.seek(current_pos)
        return -1 if found_pos == -1 else current_pos + found_pos

    def _read_and_unpack(self, fmt: str, size: int):
        data = self._read_exact(size)
        return struct.unpack(self.sign + fmt, data)

    def read_float(self) -> float:
        return self._read_and_unpack("f", 4)[0]

    def read_s128(self):
        return self._read_and_unpack("qq", 16)

    def read_u128(self):
        return self._read_and_unpack("QQ", 16)

    def read_u32(self):
        return self._read_and_unpack("I", 4)[0]
    
    def read_s32(self):
        return self._read_and_unpack("i", 4)[0]
    
    def read_s64(self):
        return self._read_and_unpack("q", 8)[0]
    
    def read_u64(self):
        return self._read_and_unpack("Q", 8)[0]
    
    def read_64bit_int(self):
        result = self.read_u64()
        if result > 0x7FFFFFFFFFFFFFFF:
            self.seek(-8, io.SEEK_CUR)
            result = self.read_s64()
        return result
    
    def read_u16(self):
        return self._read_and_unpack("H", 2)[0]
    
    def read_bool(self) -> bool:
        return self._read_and_unpack("?", 1)[0]
    
    def read_u8(self) -> int:
        return self._read_and_unpack("B", 1)[0]

    def read_string_w_size(self, exp_size: int) -> str:
        data = self._read_exact(exp_size)
        try:
            return data.decode(self.encoding)
        except UnicodeDecodeError as e:
            raise ValueError(f"Failed to decode {exp_size} bytes using {self.encoding}: {e}")

    def read_string(self) -> str | None:
        MAX_ASCII = 128
        result = bytearray()
        while True:
            byte = self._read_exact(1)
            if byte[0] > MAX_ASCII:
                self.seek(-1, io.SEEK_CUR)  # Unread the byte
                return None
            if byte == b'\0':
                break
            if len(result) >= self.max_str_size:
                raise ValueError(f"String exceeds max length ({self.max_str_size} bytes)")
            result.append(byte[0])
        return result.decode(self.encoding)
    
    @classmethod
    def from_writer(cls, writer: 'WriteStream'):
        current_pos = writer.tell()
        writer.seek(0, io.SEEK_END)
        reader = cls(writer.getvalue())
        reader.seek(0, io.SEEK_END)
        writer.seek(current_pos, io.SEEK_SET)  # Restore the original position of the writer
        return reader


class WriteStream(ReadStream):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.endian = "little"
        self.sign = "<" if self.endian == "little" else ">"
        self.encoding = "utf-8"
    
    @classmethod
    def from_reader(cls, reader:ReadStream):
        current_pos = reader.tell()
        reader.seek(0, io.SEEK_END)
        writer = cls(reader.getvalue())
        writer.seek(0, io.SEEK_END)  # Move to the end of the stream
        reader.seek(current_pos)  # Restore the original position of the reader
        return writer
    
    # def __repr__(self):
    #     return f"<WriteStream position=0x{self.tell():08X}>"
    
    # def __str__(self):
    #     return f"<WriteStream position=0x{self.tell():08X}>"
    
    def align_to(self, alignment: int = 8, pad_byte: bytes = b'\x00'):
        """Pad the stream to the next multiple of `alignment`."""
        current_pos = self.tell()
        padding = (alignment - (current_pos % alignment)) % alignment
        if padding:
            self.write(pad_byte * padding)

    def _pack_and_write(self, fmt: str, *values):
        packed = struct.pack(self.sign + fmt, *values)
        self.write(packed)

    def write_list_of_strings(self, _list: list[str]):
        """Write null-terminated strings"""
        for _s in _list:
            self.write(_s.encode(self.encoding) + b'\x00')

    def write_float(self, val: float):
        self._pack_and_write("f", val)
    
    def write_u8(self, val: int):
        self._pack_and_write("B", val)
    
    def write_s8(self, val: int):
        self._pack_and_write("b", val)
    
    def write_u16(self, val: int):
        self._pack_and_write("H", val)

    def write_s16(self, val: int):
        self._pack_and_write("h", val)

    def write_u32(self, val: int):
        self._pack_and_write("I", val)
    
    def write_s32(self, val: int):
        self._pack_and_write("i", val)

    def write_u64(self, val: int):
        self._pack_and_write("Q", val)

    def write_s64(self, val: int):
        self._pack_and_write("q", val)
        
    def write_64bit_int(self, val: int):
        """Write a 64-bit integer, signed if negative."""
        if val > 0x7FFFFFFFFFFFFFFF:
            self.write_u64(val)
        else:
            self.write_s64(val)

    def write_u128(self, val1: int, val2: int):
        """Write 128 bits as two 64-bit unsigned ints."""
        self.write_u64(val1)
        self.write_u64(val2)

    def write_s128(self, val1: int, val2: int):
        """Write 128 bits as two 64-bit signed ints."""
        self.write_s64(val1)
        self.write_s64(val2)
    
    def write_u8(self, val: int):
        self._pack_and_write("B", val)
    
    def write_bool(self, val: bool):
        self.write_u8(1 if val else 0)

    def write_bytes(self, data: bytes):
        self.write(data)

    def write_padding(self, count: int, pad_byte: bytes = b'\x00'):
        self.write(pad_byte * count)

    def write_string(self, text: str, null_terminated: bool = True):
        data = text.encode(self.encoding)
        self.write(data)
        if null_terminated:
            self.write(b'\x00')

    def write_string_w_size(self, text: str, total_size: int):
        data = text.encode(self.encoding)
        if len(data) > total_size:
            raise ValueError(f"String too long to fit in {total_size} bytes")
        self.write(data)
        self.write(b'\x00' * (total_size - len(data)))

    def _writer_align_to(self, alignment: int = 8):
        """Align the stream's position to the next multiple of `alignment`."""
        current_pos = self.tell()
        if current_pos == 0:
            return
        # size = self.get_data_size()
        pad_size = alignment - (current_pos % alignment)
        if pad_size > 0 and pad_size < alignment:
            self.write_padding(pad_size)
    
    def write_padding(self, size: int):
        self.write(b'\x00' * size)