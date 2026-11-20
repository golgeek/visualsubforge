<script lang="ts">
  import type { ReviewSession } from './review-session.svelte';
  import type { ReviewChange, ReviewThread, CommentEdit } from '../client';
  let {
    review,
    change,
    detached,
    projection = false,
  }: {
    review: ReviewSession;
    change?: ReviewChange;
    detached?: ReviewThread;
    projection?: boolean;
  } = $props();
  let composer = $state<string | null>(null);
  let text = $state('');
  let name = $state('');
  let pendingStatus = $state<CommentEdit | null>(null);
  const threads = $derived(
    detached ? [detached] : change ? review.threads(change, projection) : [],
  );
  const exported = $derived(
    (projection ? review.visibleDiscussion : review.page?.discussion)
      ?.exported ?? [],
  );
  function resolved(t: ReviewThread) {
    return (
      [...t.messages].reverse().find((m) => m.type !== 'comment')?.type ===
      'resolve'
    );
  }
  async function send(edit: CommentEdit) {
    if (await review.comment(edit, name)) {
      composer = null;
      text = '';
    }
  }
  function post() {
    if (pendingStatus) {
      void send(pendingStatus);
      pendingStatus = null;
      return;
    }
    if (composer === 'new' && change)
      void send({
        kind: 'post',
        changeKey: change.key,
        sourceIndex: (change.original[0] ?? change.revised[0])!.source_index,
        text,
      });
    else if (composer)
      void send({ kind: 'reply', threadId: composer, event: 'comment', text });
  }
</script>

{#if change}
  {#if threads.length}<span
      >{threads.length} {threads.length === 1 ? 'thread' : 'threads'}</span
    >{/if}
  <button onclick={() => (composer = 'new')}>Comment</button>
{/if}
{#each threads as t (t.id)}
  <section
    aria-label="Review thread"
    class:current={review.currentThread === t.id}
  >
    <strong>{resolved(t) ? 'Resolved' : 'Open'}</strong>
    {#each t.messages as m (m.id)}
      <p>
        <b>{m.author}</b> · <time>{new Date(m.at_ms!).toLocaleString()}</time>
      </p>
      <p class="message">
        {m.type === 'comment'
          ? m.text
          : m.type === 'resolve'
            ? 'Resolved this thread'
            : 'Reopened this thread'}
      </p>
      {#if !exported.includes(m.id) && (m.id !== t.first_message_id || t.messages.length === 1)}
        <button
          onclick={() =>
            void send({ kind: 'delete', threadId: t.id, messageId: m.id })}
          >Delete draft</button
        >
      {/if}
    {/each}
    <button onclick={() => (composer = t.id)}>Reply</button>
    <button
      onclick={() => {
        if (!review.name()) {
          pendingStatus = {
            kind: 'reply',
            threadId: t.id,
            event: resolved(t) ? 'reopen' : 'resolve',
            text: null,
          };
          composer = t.id;
          return;
        }
        void send({
          kind: 'reply',
          threadId: t.id,
          event: resolved(t) ? 'reopen' : 'resolve',
          text: null,
        });
      }}>{resolved(t) ? 'Reopen' : 'Resolve'}</button
    >
  </section>
{/each}
{#if composer}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      post();
    }}
  >
    {#if !review.name()}<label
        >Your name on review comments <input
          required
          bind:value={name}
        /></label
      ><small>Shown to everyone who receives your review files</small>{/if}
    {#if !pendingStatus}<label
        >{composer === 'new' ? 'Comment' : 'Reply'}
        <textarea required bind:value={text}></textarea></label
      >{/if}
    <button
      type="submit"
      disabled={(!pendingStatus && !text.trim()) ||
        (!review.name() && !name.trim())}>Post</button
    >
    <button
      type="button"
      onclick={() => {
        composer = null;
        pendingStatus = null;
        text = '';
      }}>Cancel</button
    >
  </form>
{/if}

<style>
  section,
  form {
    padding: 0.4rem;
    border-inline-start: 2px solid var(--line-strong);
  }
  .message {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .current {
    outline: 1px solid currentColor;
  }
  label,
  small {
    display: block;
  }
  p {
    margin: 0.2rem 0;
  }
</style>
