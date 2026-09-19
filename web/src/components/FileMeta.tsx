export function FileMeta({ name, meta }: { name: string; meta: string }) {
  return (
    <div className="file-meta">
      <div className="file-meta__name" title={name}>
        {name}
      </div>
      <div className="file-meta__line">{meta}</div>
    </div>
  )
}
