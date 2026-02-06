"""
ClaudeUsageWindow - Always-on-top widget showing real Claude.ai usage
Fetches data from claude.ai/settings/usage via Firefox cookies
"""
import tkinter as tk
from tkinter import ttk, messagebox
import base64
import json
import math
import os
import subprocess
import sys
import threading
import re
from datetime import datetime, timedelta
from pathlib import Path
import time
import urllib.request
import urllib.error

# Try to import browser_cookie3
try:
    import browser_cookie3
    HAS_BROWSER_COOKIES = True
except ImportError:
    HAS_BROWSER_COOKIES = False

# Claude logo (icons8-claude-48.png) embedded as base64
CLAUDE_LOGO_B64 = (
    "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"
    "AABJRU5ErkJggg=="
)


class TokenMonitor:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("Claude Usage")
        self.root.attributes('-topmost', True)
        self.root.overrideredirect(False)

        # Window settings
        self.root.geometry("320x200+50+50")
        self.root.resizable(False, False)
        self.root.configure(bg='#1a1a2e')

        # Set window icon
        try:
            icon_data = base64.b64decode(CLAUDE_LOGO_B64)
            self.icon_image = tk.PhotoImage(data=icon_data)
            self.root.iconphoto(True, self.icon_image)
        except Exception:
            pass

        # State - handle both .exe and .py paths
        if getattr(sys, 'frozen', False):
            base_dir = Path(sys.executable).parent
        else:
            base_dir = Path(__file__).parent
        self.state_file = base_dir / "state.json"
        self.org_uuid = None
        self.whatsapp_number = '+33XXXXXXXXX'
        self.load_state()

        # Usage data
        self.session_pct = 0
        self.session_reset = None
        self.weekly_pct = 0
        self.weekly_reset = None

        # Notification tracking
        self.session_notified = False
        self.last_session_reset = None

        # Animation state
        self._animating = False

        self.setup_ui()
        self.setup_drag()

        # Initial fetch
        self.refresh_data()

    def load_state(self):
        """Load saved state"""
        try:
            if self.state_file.exists():
                with open(self.state_file, 'r') as f:
                    data = json.load(f)
                    self.org_uuid = data.get('org_uuid')
                    self.whatsapp_number = data.get('whatsapp_number', '+33XXXXXXXXX')
                    self.session_notified = data.get('session_notified', False)
                    self.last_session_reset = data.get('last_session_reset')
        except Exception:
            pass

    def save_state(self):
        """Save state to file"""
        try:
            with open(self.state_file, 'w') as f:
                json.dump({
                    'org_uuid': self.org_uuid,
                    'whatsapp_number': self.whatsapp_number,
                    'session_notified': self.session_notified,
                    'last_session_reset': self.last_session_reset
                }, f)
        except Exception as e:
            print(f"Error saving state: {e}")

    def _draw_refresh_arrow(self, angle_offset=0, color='#ffffff'):
        """Draw a circular refresh arrow on the canvas, rotated by angle_offset degrees"""
        self.refresh_canvas.delete('all')
        s = 26
        cx, cy = s / 2, s / 2
        r = 8

        # Arc: 300 degrees of a circle
        start_deg = 30 + angle_offset
        self.refresh_canvas.create_arc(
            cx - r, cy - r, cx + r, cy + r,
            start=start_deg, extent=300,
            style='arc', outline=color, width=2
        )

        # Arrowhead at the start of the arc (the tip)
        tip_angle = math.radians(start_deg)
        tip_x = cx + r * math.cos(tip_angle)
        tip_y = cy - r * math.sin(tip_angle)

        # Tangent direction (clockwise = decreasing angle visually)
        tang_angle = tip_angle - math.pi / 2
        # A short line segment ending at the tip, with an arrowhead
        base_x = tip_x - 8 * math.cos(tang_angle)
        base_y = tip_y + 8 * math.sin(tang_angle)

        self.refresh_canvas.create_line(
            base_x, base_y, tip_x, tip_y,
            fill=color, width=2, arrow='last', arrowshape=(5, 7, 3)
        )

    def setup_ui(self):
        # Main frame
        self.main_frame = tk.Frame(self.root, bg='#1a1a2e')
        self.main_frame.pack(fill='both', expand=True, padx=8, pady=8)

        # Header - Claude logo + refresh button
        title_frame = tk.Frame(self.main_frame, bg='#16213e')
        title_frame.pack(fill='x', pady=(0, 8))

        # Load Claude logo from embedded base64
        logo_data = base64.b64decode(CLAUDE_LOGO_B64)
        self.logo_photo = tk.PhotoImage(data=logo_data)
        # Subsample from 48x48 to 24x24
        self.logo_photo = self.logo_photo.subsample(2, 2)
        logo_label = tk.Label(title_frame, image=self.logo_photo, bg='#16213e')
        logo_label.pack(side='left', padx=8, pady=4)

        # Refresh button as canvas with drawn arrow
        self.refresh_canvas = tk.Canvas(
            title_frame, width=26, height=26,
            bg='#16213e', highlightthickness=0, cursor='hand2'
        )
        self.refresh_canvas.pack(side='right', padx=4, pady=4)
        self._draw_refresh_arrow(0, '#ffffff')
        self.refresh_canvas.bind('<Button-1>', lambda e: self.refresh_with_animation())

        # Content frame
        self.content_frame = tk.Frame(self.main_frame, bg='#1a1a2e')
        self.content_frame.pack(fill='both', expand=True)

        # Style
        style = ttk.Style()
        style.theme_use('clam')
        style.configure("Session.Horizontal.TProgressbar", troughcolor='#16213e', background='#4ecca3', thickness=12)
        style.configure("Weekly.Horizontal.TProgressbar", troughcolor='#16213e', background='#4ecca3', thickness=12)

        # --- Session (5h) section ---
        tk.Label(
            self.content_frame,
            text="Session (5h)",
            bg='#1a1a2e',
            fg='#a0a0a0',
            font=('Segoe UI', 9)
        ).pack(anchor='w')

        # Session bar row: progress bar + percentage outside
        session_bar_frame = tk.Frame(self.content_frame, bg='#1a1a2e')
        session_bar_frame.pack(fill='x', pady=(2, 0))

        self.session_progress = ttk.Progressbar(
            session_bar_frame,
            style="Session.Horizontal.TProgressbar",
            mode='determinate'
        )
        self.session_progress.pack(side='left', fill='x', expand=True)

        self.session_pct_label = tk.Label(
            session_bar_frame,
            text="---%",
            bg='#1a1a2e',
            fg='#4ecca3',
            font=('Segoe UI', 10, 'bold'),
            width=5,
            anchor='e'
        )
        self.session_pct_label.pack(side='right', padx=(4, 0))

        self.session_reset_label = tk.Label(
            self.content_frame,
            text="Reset: ---",
            bg='#1a1a2e',
            fg='#666666',
            font=('Segoe UI', 8)
        )
        self.session_reset_label.pack(anchor='e', pady=(0, 8))

        # --- Weekly section ---
        tk.Label(
            self.content_frame,
            text="Hebdomadaire",
            bg='#1a1a2e',
            fg='#a0a0a0',
            font=('Segoe UI', 9)
        ).pack(anchor='w')

        # Weekly bar row: progress bar + percentage outside
        weekly_bar_frame = tk.Frame(self.content_frame, bg='#1a1a2e')
        weekly_bar_frame.pack(fill='x', pady=(2, 0))

        self.weekly_progress = ttk.Progressbar(
            weekly_bar_frame,
            style="Weekly.Horizontal.TProgressbar",
            mode='determinate'
        )
        self.weekly_progress.pack(side='left', fill='x', expand=True)

        self.weekly_pct_label = tk.Label(
            weekly_bar_frame,
            text="---%",
            bg='#1a1a2e',
            fg='#4ecca3',
            font=('Segoe UI', 10, 'bold'),
            width=5,
            anchor='e'
        )
        self.weekly_pct_label.pack(side='right', padx=(4, 0))

        self.weekly_reset_label = tk.Label(
            self.content_frame,
            text="Reset: ---",
            bg='#1a1a2e',
            fg='#666666',
            font=('Segoe UI', 8)
        )
        self.weekly_reset_label.pack(anchor='e', pady=(0, 8))

        # Status label
        self.status_label = tk.Label(
            self.content_frame,
            text="",
            bg='#1a1a2e',
            fg='#666666',
            font=('Segoe UI', 8)
        )
        self.status_label.pack(pady=(4, 0))

    def setup_drag(self):
        """Enable window dragging"""
        self.root.bind('<Button-1>', self.start_drag)
        self.root.bind('<B1-Motion>', self.do_drag)

    def start_drag(self, event):
        self.drag_x = event.x
        self.drag_y = event.y

    def do_drag(self, event):
        x = self.root.winfo_x() + event.x - self.drag_x
        y = self.root.winfo_y() + event.y - self.drag_y
        self.root.geometry(f"+{x}+{y}")

    def refresh_with_animation(self):
        """Refresh with the circular arrow spinning 360 degrees once"""
        if self._animating:
            return
        self._animating = True

        step = 15  # degrees per frame
        total = 360
        delay = 20  # ms between frames

        def animate(angle=0):
            if angle < total:
                self._draw_refresh_arrow(angle, '#4ecca3')
                self.root.after(delay, lambda: animate(angle + step))
            else:
                self._draw_refresh_arrow(0, '#ffffff')
                self._animating = False

        animate()
        self.refresh_data()

    def refresh_data(self):
        """Fetch usage data in background"""
        self.status_label.config(text="Chargement...", fg='#666666')
        threading.Thread(target=self._fetch_usage, daemon=True).start()

    def _get_cookies(self):
        """Get cookies from Firefox"""
        if not HAS_BROWSER_COOKIES:
            return None
        try:
            cj = browser_cookie3.firefox(domain_name='claude.ai')
            return '; '.join([f'{c.name}={c.value}' for c in cj])
        except Exception as e:
            print(f"Cookie error: {e}")
            return None

    def _get_org_uuid(self, cookies):
        """Fetch organization UUID"""
        if self.org_uuid:
            return self.org_uuid

        try:
            url = 'https://claude.ai/api/organizations'
            req = urllib.request.Request(url)
            req.add_header('Cookie', cookies)
            req.add_header('Accept', 'application/json')
            req.add_header('User-Agent', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)')

            with urllib.request.urlopen(req, timeout=15) as resp:
                data = json.loads(resp.read().decode('utf-8'))
                if data and len(data) > 0:
                    self.org_uuid = data[0]['uuid']
                    self.save_state()
                    return self.org_uuid
        except Exception as e:
            print(f"Org fetch error: {e}")
        return None

    def _fetch_usage(self):
        """Fetch usage from Claude.ai API"""
        try:
            if not HAS_BROWSER_COOKIES:
                self._update_error("pip install browser-cookie3")
                return

            cookies = self._get_cookies()
            if not cookies:
                self._update_error("Ouvre Firefox sur claude.ai")
                return

            org_uuid = self._get_org_uuid(cookies)
            if not org_uuid:
                self._update_error("Org non trouvée")
                return

            # Fetch usage
            url = f'https://claude.ai/api/organizations/{org_uuid}/usage'
            req = urllib.request.Request(url)
            req.add_header('Cookie', cookies)
            req.add_header('Accept', 'application/json')
            req.add_header('Origin', 'https://claude.ai')
            req.add_header('Referer', 'https://claude.ai/settings/usage')
            req.add_header('User-Agent', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)')

            with urllib.request.urlopen(req, timeout=15) as resp:
                data = json.loads(resp.read().decode('utf-8'))
                self._process_usage(data)

        except urllib.error.HTTPError as e:
            if e.code == 403:
                self._update_error("Session expirée - reconnecte-toi sur claude.ai")
            else:
                self._update_error(f"Erreur HTTP {e.code}")
        except Exception as e:
            self._update_error(f"Erreur: {str(e)[:30]}")

    def _process_usage(self, data):
        """Process usage data and update UI"""
        five_hour = data.get('five_hour', {})
        seven_day = data.get('seven_day', {})

        self.session_pct = five_hour.get('utilization', 0) or 0
        self.weekly_pct = seven_day.get('utilization', 0) or 0

        session_reset_str = five_hour.get('resets_at')
        weekly_reset_str = seven_day.get('resets_at')

        # Parse reset times
        if session_reset_str:
            self.session_reset = datetime.fromisoformat(session_reset_str.replace('Z', '+00:00'))
        if weekly_reset_str:
            self.weekly_reset = datetime.fromisoformat(weekly_reset_str.replace('Z', '+00:00'))

        # Check if session reset changed (new session available)
        if session_reset_str != self.last_session_reset:
            if self.last_session_reset is not None and self.session_pct < 10:
                self._send_notification()
            self.last_session_reset = session_reset_str
            self.session_notified = False
            self.save_state()

        # Schedule notification if usage is high
        if self.session_pct >= 95 and not self.session_notified:
            self._schedule_notification()

        self._update_ui()

    def _update_ui(self):
        """Update the UI from main thread"""
        def update():
            # Session
            self.session_pct_label.config(text=f"{self.session_pct:.0f}%")
            self.session_progress['value'] = self.session_pct

            if self.session_reset:
                local_reset = self.session_reset.astimezone()
                self.session_reset_label.config(text=f"Reset: {local_reset.strftime('%H:%M')}")

            # Color coding
            if self.session_pct >= 90:
                self.session_pct_label.config(fg='#e94560')
                style = ttk.Style()
                style.configure("Session.Horizontal.TProgressbar", background='#e94560')
            elif self.session_pct >= 70:
                self.session_pct_label.config(fg='#f4a261')
                style = ttk.Style()
                style.configure("Session.Horizontal.TProgressbar", background='#f4a261')
            else:
                self.session_pct_label.config(fg='#4ecca3')
                style = ttk.Style()
                style.configure("Session.Horizontal.TProgressbar", background='#4ecca3')

            # Weekly
            self.weekly_pct_label.config(text=f"{self.weekly_pct:.0f}%")
            self.weekly_progress['value'] = self.weekly_pct

            if self.weekly_reset:
                local_reset = self.weekly_reset.astimezone()
                self.weekly_reset_label.config(text=f"Reset: {local_reset.strftime('%d/%m %H:%M')}")

            if self.weekly_pct >= 90:
                self.weekly_pct_label.config(fg='#e94560')
                style = ttk.Style()
                style.configure("Weekly.Horizontal.TProgressbar", background='#e94560')
            elif self.weekly_pct >= 70:
                self.weekly_pct_label.config(fg='#f4a261')
                style = ttk.Style()
                style.configure("Weekly.Horizontal.TProgressbar", background='#f4a261')
            else:
                self.weekly_pct_label.config(fg='#4ecca3')
                style = ttk.Style()
                style.configure("Weekly.Horizontal.TProgressbar", background='#4ecca3')

            # Status
            now = datetime.now().strftime('%H:%M:%S')
            self.status_label.config(text=f"Mis à jour: {now}", fg='#4ecca3')

        self.root.after(0, update)

    def _update_error(self, msg):
        """Update UI with error message"""
        def update():
            self.status_label.config(text=msg, fg='#e94560')
        self.root.after(0, update)

    def _schedule_notification(self):
        """Schedule a WhatsApp notification for when session resets"""
        if self.session_reset and not self.session_notified:
            self.session_notified = True
            self.save_state()

            now = datetime.now(self.session_reset.tzinfo)
            delay = (self.session_reset - now).total_seconds()

            if delay > 0:
                def notify_later():
                    time.sleep(delay)
                    self._send_notification()

                threading.Thread(target=notify_later, daemon=True).start()

    def _send_notification(self):
        """Send WhatsApp notification via OpenClaw"""
        try:
            message = "Tes tokens Claude sont de nouveau disponibles! Session réinitialisée."

            cmd = [
                'openclaw', 'message', 'send',
                '--channel', 'whatsapp',
                '--target', self.whatsapp_number,
                '--message', message
            ]

            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
            )

            if result.returncode == 0:
                print("WhatsApp notification sent!")
            else:
                print(f"WhatsApp error: {result.stderr}")

        except Exception as e:
            print(f"Notification error: {e}")

    def run(self):
        # Auto-refresh every 2 minutes
        def auto_refresh():
            self.refresh_data()
            self.root.after(120000, auto_refresh)

        self.root.after(120000, auto_refresh)
        self.root.mainloop()


if __name__ == "__main__":
    app = TokenMonitor()
    app.run()
