

def check_if_bytes_is_utf8(data:bytes):
    try:
        data.decode("utf-8")
        return True
    except Exception as e:
        return False