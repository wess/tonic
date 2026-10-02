"""Strip docs/specs from Elixir sources, keeping line numbers intact.

Removed lines become blank lines so stack traces point at the same lines as
the original Elixir sources. A provenance comment is appended at the end.

  python3 strip_docs.py OUT SRC [ORIGIN]
"""
import re, sys


def strip(s):
    lines = s.split('\n')
    out = []
    i = 0
    while i < len(lines):
        l = lines[i]
        if re.match(r'^\s*@(moduledoc|doc|typedoc)\s+(~[sS])?"""\s*$', l):
            out.append('')
            i += 1
            while not re.match(r'^\s*"""\s*$', lines[i]):
                out.append('')
                i += 1
            out.append('')
            i += 1
            continue
        if re.match(r'^\s*@(moduledoc|doc|typedoc)\b', l):
            out.append('')
            i += 1
            continue
        m = re.match(r'^(\s*)@(spec|type|typep|opaque|callback|macrocallback|impl|compile|dialyzer|deprecated)\b', l)
        if m:
            ind = len(m.group(1))
            out.append('')
            i += 1
            # continuation lines: more indented than attribute
            while i < len(lines) and lines[i].strip() and (len(lines[i]) - len(lines[i].lstrip())) > ind:
                out.append('')
                i += 1
            continue
        out.append(l)
        i += 1
    return '\n'.join(out)


src = sys.argv[2]
origin = sys.argv[3] if len(sys.argv) > 3 else src
text = strip(open(src).read()).rstrip('\n')
text += '\n\n# Imported from Elixir 1.18.3 lib/elixir/lib/%s (docs and specs stripped;\n# line numbers match the original).\n' % origin
open(sys.argv[1], 'w').write(text)
