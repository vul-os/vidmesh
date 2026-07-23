import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Avatar, TimeAgo } from "@evermesh/ui";
import { useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { postComment } from "../api.js";
import { useMe } from "../hooks/useMe.js";
import type { Comment } from "../lib/api-types.js";
import { buildCommentTree, countComments, type CommentNode } from "../lib/commentTree.js";

export interface CommentThreadProps {
  videoId: string;
  comments: Comment[];
}

/** Recursive, collapsible comment tree with per-node reply forms. */
export function CommentThread({ videoId, comments }: CommentThreadProps): JSX.Element {
  const tree = buildCommentTree(comments);
  const total = countComments(tree);

  return (
    <div>
      <h2 className="mb-3 text-lg font-semibold">{total} comments on this gateway</h2>
      <ReplyForm videoId={videoId} parent={null} autoFocus={false} />
      {tree.length === 0 ? (
        <p role="status" className="py-6 text-sm text-muted">
          No comments yet on this gateway. Be the first.
        </p>
      ) : (
        <ul className="mt-5 space-y-5">
          {tree.map((node) => (
            <li key={node.comment.id}>
              <CommentNodeView videoId={videoId} node={node} depth={0} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function CommentNodeView({ videoId, node, depth }: { videoId: string; node: CommentNode; depth: number }): JSX.Element {
  const [collapsed, setCollapsed] = useState(false);
  const [replying, setReplying] = useState(false);
  const hasChildren = node.children.length > 0;

  return (
    <div className={depth > 0 ? "border-l-2 border-line pl-4" : undefined}>
      <div className="flex gap-2.5">
        <Avatar name={node.comment.author.name} size="sm" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-2">
            <span className="font-medium text-ink">{node.comment.author.name}</span>
            <TimeAgo unixMs={node.comment.createdAt * 1000} className="text-xs text-faint" />
          </div>
          <p className="whitespace-pre-wrap break-words text-sm text-ink">{node.comment.text}</p>
          <div className="mt-1 flex gap-3 text-xs font-medium">
            <button type="button" onClick={() => setReplying((v) => !v)} className="text-signal hover:underline">
              Reply
            </button>
            {hasChildren && (
              <button type="button" onClick={() => setCollapsed((v) => !v)} aria-expanded={!collapsed} className="text-muted hover:underline">
                {collapsed ? `Show ${node.children.length} replies` : "Hide replies"}
              </button>
            )}
          </div>
          {replying && <ReplyForm videoId={videoId} parent={node.comment.id} autoFocus onDone={() => setReplying(false)} />}
        </div>
      </div>

      {hasChildren && !collapsed && (
        <ul className="mt-3 space-y-3">
          {node.children.map((child) => (
            <li key={child.comment.id}>
              <CommentNodeView videoId={videoId} node={child} depth={depth + 1} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function ReplyForm({
  videoId,
  parent,
  autoFocus,
  onDone,
}: {
  videoId: string;
  parent: string | null;
  autoFocus: boolean;
  onDone?: () => void;
}): JSX.Element {
  const { data: me } = useMe();
  const [text, setText] = useState("");
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: (body: { text: string; parent?: string }) => postComment(videoId, body),
    onSuccess: () => {
      setText("");
      onDone?.();
      void queryClient.invalidateQueries({ queryKey: ["comments", videoId] });
    },
  });

  if (!me) {
    return (
      <p className="mt-2 text-sm text-muted">
        <Link to="/auth" className="text-signal hover:underline">
          Sign in
        </Link>{" "}
        to comment.
      </p>
    );
  }

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!text.trim()) return;
    mutation.mutate(parent ? { text, parent } : { text });
  };

  return (
    <form onSubmit={onSubmit} className="mt-2 flex gap-2">
      <label htmlFor={`reply-${parent ?? "root"}`} className="sr-only">
        {parent ? "Write a reply" : "Write a comment"}
      </label>
      <textarea
        id={`reply-${parent ?? "root"}`}
        value={text}
        onChange={(e) => setText(e.target.value)}
        autoFocus={autoFocus}
        rows={2}
        placeholder={parent ? "Write a reply…" : "Write a comment…"}
        className="vm-field flex-1 resize-y"
      />
      <button type="submit" disabled={mutation.isPending || !text.trim()} className="vm-btn vm-btn-primary self-start">
        Post
      </button>
      {mutation.isError && (
        <p role="alert" className="text-xs text-red-700 dark:text-red-300">
          {mutation.error instanceof Error ? mutation.error.message : "Failed to post."}
        </p>
      )}
    </form>
  );
}
