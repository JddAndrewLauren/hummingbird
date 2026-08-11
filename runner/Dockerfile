# The skill-runner endpoint (#41, #256): a scale-to-zero Fly app, separate
# from the sweeper (docs/sweeper.md forbids [http_service] there). The image
# bakes in the Claude Code CLI, the skills this build ships, and the runner
# server itself -- a skill change ships by `fly deploy`, the image *is* the
# skill version (#41 decision 5).
FROM node:22-slim

# The Claude Code CLI, installed globally so `claude` is on PATH for
# child_process.spawn inside the runner.
RUN npm install -g @anthropic-ai/claude-code

WORKDIR /app

# v1 ships parse-capture only (#256, 2026-08-10 decision) -- next-up-personal
# and microtask wait behind the write-target decision, so their skill
# directories (and scripts/linear.sh) are deliberately NOT baked in yet.
# Widening this COPY is the whole change needed to add the next op.
COPY .claude/skills/parse-capture ./.claude/skills/parse-capture

COPY runner/package.json ./runner/package.json
COPY runner/src ./runner/src

WORKDIR /app/runner

EXPOSE 8080

CMD ["node", "src/main.js"]
