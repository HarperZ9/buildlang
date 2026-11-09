Generator prompt (sent verbatim to each fresh subagent; `<SAMPLE>` is A or B)

You are writing programs in BuildLang, a programming language you may not know.
Your only sources are these five files. Read them and nothing else:

- <PROBE>/TASKS.md (the tasks)
- <PROBE>/context/SKILL.md
- <PROBE>/context/llms.txt
- <PROBE>/context/SPECIFICATION.md
- <PROBE>/context/EFFECTS_GUIDE.md

Rules:
- Do not read any other file or directory, do not search the disk, do not use
  the web, and do not run any program or shell command (no compiler, no
  buildc, no tests). Work only from the five files above.
- Write one answer per task with the file-write tool, to
  <PROBE>/gen/<SAMPLE>/<id>.bld, where <id> is the task id from TASKS.md
  (34 files). Write each file once; this is a single attempt per task.
- When done, reply with: the list of files written, which files you read, and
  any rule you broke or were unsure about. Do not paste the programs.

<PROBE> is the probe folder named in the message that carries this prompt.
