"""Client for the OpenMasterHub local daemon API.

See docs/protocol.md. Plugins open this client themselves.
"""

from openmasterhub.client import MasterHubClient, MasterHubError

__all__ = ["MasterHubClient", "MasterHubError"]
__version__ = "0.1.0"
