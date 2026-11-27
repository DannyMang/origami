export function Clip({ name, className }: { name: string; className?: string }) {
  return (
    <video
      className={className}
      src={`/media/${name}.mp4`}
      poster={`/media/${name}.jpg`}
      autoPlay
      loop
      muted
      playsInline
    />
  );
}
