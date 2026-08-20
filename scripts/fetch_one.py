import sys, json, urllib.request

opcode = sys.argv[1].lower()
url = f"https://raw.githubusercontent.com/SingleStepTests/65x02/main/6502/v1/{opcode}.json"
out = sys.argv[2]
n = int(sys.argv[3]) if len(sys.argv) > 3 else 15

try:
    with urllib.request.urlopen(url, timeout=30) as resp:
        data = json.load(resp)
except Exception as e:
    print(f"SKIP {opcode}: {e}")
    sys.exit(0)

subset = data[:n]
with open(out, "w") as f:
    json.dump(subset, f)
print(f"OK {opcode}: {len(subset)} cases")
