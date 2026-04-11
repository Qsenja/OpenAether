from registry import registry

# Auto-Load all consolidated skills
import skills.agent
import skills.system
import skills.desktop
import skills.web

# Expose registry for main.py (main.py does 'from tools import registry')
__all__ = ["registry"]
