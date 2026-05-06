---
tags: [building]
source: mixed
---
# Getting Started

```bash
git clone https://github.com/Reflective-Lab/embassy-ports.git
cd embassy-ports
just                # check + lint + test
```

For a release-grade verification, run the full ritual:

```bash
just release-check
```

See [[Building/Release Commands]] for what each gate does.
