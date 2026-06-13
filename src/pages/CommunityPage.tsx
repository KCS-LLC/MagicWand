export function CommunityPage() {
  return (
    <div className="page-view">
      <header className="header">
        <h1>Community Trainers</h1>
      </header>
      <p className="page-description">
        Community trainers are JSON files anyone can write and share.
        Drop a trainer file into the trainers folder and it will appear in your Library automatically.
        Visit the project repository to learn how to write and submit your own trainers.
      </p>
      <div className="community-grid">
        <div className="community-card coming-soon">
          <span className="game-icon">🎮</span>
          <p>The Witcher 3</p>
          <span className="coming-soon-label">Coming Soon</span>
        </div>
        <div className="community-card coming-soon">
          <span className="game-icon">🎮</span>
          <p>Elden Ring</p>
          <span className="coming-soon-label">Coming Soon</span>
        </div>
        <div className="community-card coming-soon">
          <span className="game-icon">🎮</span>
          <p>Cyberpunk 2077</p>
          <span className="coming-soon-label">Coming Soon</span>
        </div>
      </div>
    </div>
  );
}
