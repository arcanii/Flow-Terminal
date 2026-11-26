# The providers' own lists of models

Asked for on 2026-11-27: `/model` lists the models written into Flow
(`Provider::models()`, three a provider), so a model Anthropic, OpenAI or
Google release reaches Flow only with Flow's next release. The user chose
to have Flow ask each provider for its list instead.

## What it does

- **Where the list comes from.** Each provider has an endpoint listing the
  models a key may use, which Flow already calls to check a key: Claude's
  and GPT's `GET /v1/models`, Gemini's `GET /v1beta/models`. `ai/models.rs`
  fetches it (`models::fetch`), every page of it, on a `flow-models`
  thread, with the key the assistant holds (`Agent::key`) and the
  endpoint it talks to (an `*_BASE_URL` in the environment counts, as for
  everything else), and hands it back as `UserEvent::ModelsListed`.
- **When.** As `/model` opens its list or looks for words, for the
  provider in use alone, where Flow holds its key, and no more than once a
  day (`models::KEPT_FOR`); a list that didn't come is asked for again
  after `models::AGAIN_AFTER`. It's kept in memory (`AiPanel::live`), never
  on disk, and snapshots never ask. The other providers' rows are Flow's
  own: their keys aren't read until they're chosen, so no Keychain prompt
  comes for them.
- **What's shown.** Flow's own models first, as before, with their short
  names and their order; then the provider's models Flow doesn't list,
  in the provider's order (Anthropic's and OpenAI's newest first), each
  under the provider's own name for it where it gives one (Anthropic's
  `display_name`, Gemini's `displayName`; GPT's id), marked as from the
  provider's list. While it's asked for, the list's title says so; where
  it didn't come, it says the provider didn't answer, and Flow's own stay.
- **What's left out.**
  - A dated copy of a model listed already: an id that's another's and
    `-20261001`, `-2026-10-01` or `-001` (`models::snapshot_of`), as the
    providers list their snapshots beside their names.
  - OpenAI's models that can't hold a conversation: embeddings, speech,
    transcription, images, moderation, realtime and search
    (`models::chat_model`), and any not named `gpt-…` or `o<digit>…`.
  - Gemini's that can't `generateContent`.
- **Choosing.** A listed model is chosen as Flow's are: ↑↓ and Enter, a
  click, its number, or `/model` with its id or words of its name
  (`provider::find_model` over the whole list). Words that name none
  while the provider's list hasn't come yet wait for it
  (`App::pending_model_words`), then are looked for again, so `/model
  claude-opus-6` works the moment Anthropic lists it. A model picked goes
  into config.toml as Settings' Model would put it, and Settings' Model
  row keeps one that isn't Flow's selectable, as it did one set by hand.
- **The list moving.** Rows the provider's list adds come after the
  provider in use's own, so the numbers of the other providers' models
  move once it comes; the row chosen stays on its model.

## Limits

- A model Flow doesn't know may want a request setting Flow doesn't send
  yet (a new kind of reasoning, a parameter renamed): its request fails
  with the provider's error, as a mistyped model in config.toml does, and
  Flow's next release adds what it needs. Its rows say they're from the
  provider's list, so that's no surprise.
- Short names, the order of Flow's own and the defaults stay in
  `Provider::models()`, which a release still changes.
