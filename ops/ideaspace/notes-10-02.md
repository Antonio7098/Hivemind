Notes 07 02 Hivemind

Rituntime hardening- handling rate limiting, errors, ect... With fallbacks, retries, ect...
How d we deal with the interactive question asking, permissions, ect.... in a robust way
Projects linked to GitHub repo, and have a local path to main worktree.
In plan mode, spawn runtime in special directory where I can discover commands/docs, and run system wide Hivemind commands to create/edit projects, flows, ect...
Freeflow, plan, workers (each one has templates- system prompt, sills, capabilities)
How can we make editing projects ect safe and undoable?
We can instruct agent to make a Todo list, commit after every completion, and we can parse the Todo list from stout for status updates. More broadly, project stout to hivemind events. 
You can pause task execution, edit the task, and then continue/restart
CLI command to nicely display project/graph/TaskFlow/ for human/llm in md.
Hivemind is NOT for vibe coding. Use Hivemind when you require structure, replayability, verification, safe parralisatio
Ideas flow freely. Plans execute deterministically 
Structured flow
Complete tag. A task can be auto complete (completes on success), manual complete, or complete override (complete even without success). Complete " closed, things that have it as a dep can proceed
Click on button on task "edit with plan agent"
Scope root- where the runtime is spawned
project principles/rules- things that are enforced in the validation stage
24h- fully discoverable through cli. need to make that a clear goal. also supply actual docs? and make it clear they are to identigy gaps/inconsistencies.
Ethan.smithh toons