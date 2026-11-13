---
name: klistra
description: Plan short videos and pictures in the person's klistra projects with the klistra tools - list or make projects, write and edit the plan, add media from this computer, build, check and save renders. Use when the person asks for a video, an ad, a social clip, a banner or a poster made with klistra, or mentions a klistra project.
---

klistra makes short videos and pictures from a plan. With the klistra tools you plan with the person, in their projects; klistra's builder then makes the video or picture from the plan and renders it. The person sees every edit you make in the klistra studio at once, and can undo it there.

- Find the project with `list_projects`, or make one with `create_project`. Then call `get_project` before you edit, and again when the person may have changed something in the studio.
- Connect to a chat of the project with `connect_chat` before you change anything: edits are refused until you are. The person then sees there what you do, and what they write in that chat, or comment on a render, comes to you here as a message from klistra. Answer them there with `reply`. When the work is done, `disconnect_chat`, and klistra's own planner answers in that chat again.
- Bring in what is on this computer: pictures, footage, logos, screenshots, recordings. `upload_media` gives a shell command that sends the files; run it, then call `finish_uploads`. Look at media with `view_media` before you write about it.
- Building (`send_to_builder`), making media (`generate_media`) and reading websites cost the team's credits: do them when the person asks, or say first what you are about to do. After a build, `wait_for_build` (a build often takes 10 to 30 minutes; it waits until the build ends), check the result with `view_frame`, and say briefly what changed. `download_render` saves a render here.
- The klistra server's instructions say how a good plan is written: follow them.

If a klistra tool says this computer is not connected, use the login skill (/klistra:login).
