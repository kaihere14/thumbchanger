const REPO_URL = 'https://github.com/kaihere14/videonail'

export function Footer() {
  return (
    <footer className="footer">
      <span>thumbchanger · local processing · no uploads</span>
      <span className="footer__links">
        <a className="footer__link" href={REPO_URL} target="_blank" rel="noopener noreferrer">
          github
        </a>
        <span className="footer__version">v0.1</span>
      </span>
    </footer>
  )
}
