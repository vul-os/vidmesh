import { useMutation, useQuery } from "@tanstack/react-query";
import { UploadCloudIcon } from "@evermesh/ui";
import { useState, type ChangeEvent, type DragEvent, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { getUploadStatus, upload } from "../api.js";
import { useMe } from "../hooks/useMe.js";

const LICENSES = ["all-rights-reserved", "cc-by", "cc-by-sa", "cc-by-nc", "cc0", "public-domain"];

export function Upload(): JSX.Element {
  const { data: me, isLoading: meLoading } = useMe();

  if (meLoading) {
    return (
      <p role="status" className="py-10 text-sm text-muted">
        Loading…
      </p>
    );
  }
  if (!me) {
    return (
      <p role="alert" className="vm-card px-6 py-10 text-center text-sm text-muted">
        <Link to="/auth" className="font-medium text-signal hover:underline">
          Sign in
        </Link>{" "}
        to upload a video.
      </p>
    );
  }
  return <UploadForm />;
}

function UploadForm(): JSX.Element {
  const [file, setFile] = useState<File | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [tags, setTags] = useState("");
  const [license, setLicense] = useState(LICENSES[0]);
  // GAP: API.md's upload form takes a freeform `channelId`, but there's
  // no endpoint listing "my channels" to populate a dropdown with. Left
  // as a plain optional text input until the contract adds one.
  const [channelId, setChannelId] = useState("");
  const [uploadId, setUploadId] = useState<string | null>(null);

  const uploadMutation = useMutation({
    mutationFn: () => {
      if (!file) throw new Error("choose a file first");
      const tagList = tags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      return upload(file, { title, description: description || undefined, tags: tagList.length ? tagList : undefined, channelId: channelId || undefined, license });
    },
    onSuccess: (res) => setUploadId(res.uploadId),
  });

  const statusQuery = useQuery({
    queryKey: ["upload", uploadId],
    queryFn: () => getUploadStatus(uploadId as string),
    enabled: Boolean(uploadId),
    refetchInterval: (query) => (query.state.data?.status === "processing" ? 1500 : false),
  });

  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setDragOver(false);
    const dropped = e.dataTransfer.files[0];
    if (dropped) setFile(dropped);
  };

  const onFileInput = (e: ChangeEvent<HTMLInputElement>) => {
    setFile(e.target.files?.[0] ?? null);
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!file || !title.trim()) return;
    uploadMutation.mutate();
  };

  const done = statusQuery.data?.status === "published" || statusQuery.data?.status === "failed";

  return (
    <div className="mx-auto max-w-xl">
      <h1 className="mb-5 text-xl font-semibold">Upload a video</h1>

      {!uploadId || !done ? (
        <form onSubmit={onSubmit} className="vm-card space-y-4 p-5">
          <div
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(true);
            }}
            onDragLeave={() => setDragOver(false)}
            onDrop={onDrop}
            className={`rounded-control border-2 border-dashed p-8 text-center transition-colors duration-150 ${dragOver ? "border-accent-500 bg-accent-50 dark:bg-accent-950" : "border-line-strong bg-surface-2/40"}`}
          >
            <UploadCloudIcon size={28} className="mx-auto mb-2 text-muted" />
            <label htmlFor="file-input" className="block cursor-pointer text-sm">
              {file ? (
                <span className="font-medium text-ink">Selected: {file.name}</span>
              ) : (
                <>
                  <span className="font-medium text-signal">Choose a file</span>{" "}
                  <span className="text-muted">or drag and drop a video here</span>
                </>
              )}
            </label>
            <input id="file-input" type="file" accept="video/*" onChange={onFileInput} className="sr-only" />
          </div>

          <label className="vm-label">
            Title
            <input required value={title} onChange={(e) => setTitle(e.target.value)} className="vm-field" />
          </label>

          <label className="vm-label">
            Description
            <textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={3} className="vm-field resize-y" />
          </label>

          <label className="vm-label">
            Tags (comma-separated)
            <input value={tags} onChange={(e) => setTags(e.target.value)} className="vm-field" />
          </label>

          <label className="vm-label">
            License
            <select value={license} onChange={(e) => setLicense(e.target.value)} className="vm-field">
              {LICENSES.map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
            </select>
          </label>

          <label className="vm-label">
            Channel id (optional)
            <input value={channelId} onChange={(e) => setChannelId(e.target.value)} className="vm-field" />
          </label>

          <button type="submit" disabled={!file || !title.trim() || uploadMutation.isPending} className="vm-btn vm-btn-primary w-full">
            {uploadMutation.isPending ? "Uploading…" : "Upload"}
          </button>

          {uploadMutation.isError && (
            <p role="alert" className="text-sm text-red-700 dark:text-red-300">
              {uploadMutation.error instanceof Error ? uploadMutation.error.message : "Upload failed."}
            </p>
          )}
        </form>
      ) : null}

      {uploadId && (
        <div className="vm-card mt-6 p-5" role="status" aria-live="polite">
          <p className="text-sm font-medium">Processing status: {statusQuery.data?.status ?? "checking…"}</p>
          {typeof statusQuery.data?.progress === "number" && (
            <div className="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-surface-2">
              <div
                className="h-full rounded-full bg-accent-500 transition-[width] duration-300 ease-vm"
                style={{ width: `${Math.round(statusQuery.data.progress * 100)}%` }}
              />
            </div>
          )}
          {statusQuery.data?.status === "published" && statusQuery.data.manifestId && (
            <p className="mt-2 text-sm">
              Published.{" "}
              <Link to={`/watch/${encodeURIComponent(statusQuery.data.manifestId)}`} className="font-medium text-signal hover:underline">
                Watch it now
              </Link>
              .
            </p>
          )}
          {statusQuery.data?.status === "failed" && (
            <p role="alert" className="mt-2 text-sm text-red-700 dark:text-red-300">
              {statusQuery.data.error ?? "Upload processing failed."}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
