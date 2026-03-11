import sys

def hello(name: str) -> str:
    return f'hello:{name}'

if __name__ == '__main__':
    arg = sys.argv[1] if len(sys.argv) > 1 else 'world'
    print(hello(arg))
