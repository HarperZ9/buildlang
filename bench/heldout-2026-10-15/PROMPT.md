Generator prompt (sent verbatim to each fresh subagent; `<SAMPLE>` is A or B)

You are writing programs in BuildLang, a programming language you may not know.
Your only sources are these five files. Read them and nothing else:

- C:/dev/mission-control/2026-10-14-buildlang/bench4/TASKS.md (the tasks)
- C:/dev/mission-control/2026-10-14-buildlang/bench4/context/SKILL.md
- C:/dev/mission-control/2026-10-14-buildlang/bench4/context/llms.txt
- C:/dev/mission-control/2026-10-14-buildlang/bench4/context/SPECIFICATION.md
- C:/dev/mission-control/2026-10-14-buildlang/bench4/context/EFFECTS_GUIDE.md

Rules:
- Do not read any other file or directory, do not search the disk, do not use
  the web, and do not run any program or shell command (no compiler, no
  buildc, no tests). Work only from the five files above.
- Write one answer per task with the file-write tool, to
  C:/dev/mission-control/2026-10-14-buildlang/bench4/gen/<SAMPLE>/<id>.bld,
  where <id> is the task id from TASKS.md (27 files). Write each file once;
  this is a single attempt per task.
- When done, reply with: the list of files written, which files you read, and
  any rule you broke or were unsure about. Do not paste the programs.

Repair prompt (secondary metric; one fresh subagent for all failed tasks of
sample A, sent after grading):

You are fixing BuildLang programs. Read only the five files listed above, the
first answers in C:/dev/mission-control/2026-10-14-buildlang/bench4/gen/A/,
and the grader reports in
C:/dev/mission-control/2026-10-14-buildlang/bench4/repair-input/<id>.txt (one
per failed task: the checker JSON report, the build error, the crash output or
the output difference). The same rules apply: read nothing else, run nothing.
For each failed task write one revised program to
C:/dev/mission-control/2026-10-14-buildlang/bench4/gen/A-repair/<id>.bld,
once. Reply with the files written, the files read and any rule you broke.
