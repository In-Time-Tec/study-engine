# Roadmap

study-engine today is a deliberately small thing: an FSRS scheduler, a SQLite
store, a CLI, and a web UI for answering multiple-choice questions. It does that
one job well. This document sketches where it could go. Nothing below is a
promise, and some of it may turn out to be the wrong idea once tried.

## The shape of the bet

A multiple-choice quizzer tests recognition. Real understanding shows up when
you have to explain, apply, and get something wrong in a way that teaches you.
The interesting direction is to keep the engine deterministic and dumb on
purpose, and let a language model supply the part that benefits from judgment:
the conversation.

The split worth preserving:

- **The engine stays small and deterministic.** Scheduling, grading, what is due
  today, what the learner has and has not mastered, the full history. These are
  things you want to be exactly reproducible and never improvised. SQLite is the
  memory; the CLI is the only thing that writes to it.
- **The model supplies pedagogy, not bookkeeping.** Asking the next question,
  reading a free-text answer, deciding whether an explanation reveals too much,
  choosing what to review next. A tutor that can hold a conversation, not just
  flip a card.

Keeping that boundary clean is the whole game. If the model owned the memory,
sessions would drift; if the engine tried to own the teaching, it would be rigid.

## Candidate capabilities

In rough order of how soon they seem worth trying:

- **Free-text answers, graded against a rubric.** Let the learner write a real
  answer and have it scored on substance rather than letter choice, while the
  FSRS rating still flows from whether they got it right.
- **Hint ladders and gated reveals.** Surface a graded series of hints before the
  answer, and make "do not show the explanation yet" something the engine can
  enforce rather than something the model is merely asked to honor.
- **A concept graph.** Tag questions into prerequisite relationships so a weak
  spot can pull in the concepts underneath it. SQLite recursive queries make this
  cheap.
- **Mastery with forgetting.** Track per-concept mastery that decays over time, so
  the dashboard reflects what you actually still know, not what you once answered
  correctly.
- **Ingesting source material.** Turn a chapter, a spec, or a set of notes into a
  question bank automatically, so the tool is not limited to hand-written banks.
- **Session rituals.** A start-of-session orientation and an end-of-session
  consolidation that summarize what moved and what to revisit.
- **A multi-bank UI.** The backend is already certification-agnostic; the web UI
  still assumes a single bank. A bank switcher would make that generality visible.

## What is unsolved

- How to grade free text fairly and consistently without the rating becoming a
  coin flip.
- How to enforce "do not reveal the answer" against a model that is helpful by
  default.
- How to keep the model's contribution auditable, so a study session is something
  you can trust and reconstruct later.

These are the questions that decide whether the larger idea is worth building.
Until they have good answers, the small, reliable quizzer is the product.
