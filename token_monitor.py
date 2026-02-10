"""
ClaudeUsageWindow - Always-on-top widget showing real Claude.ai usage
Fetches data from claude.ai/settings/usage via Firefox cookies
"""
import tkinter as tk
from tkinter import ttk, messagebox
import json
import os
import subprocess
import threading
import re
from datetime import datetime, timedelta, timezone
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


class TokenMonitor:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("Claude Usage")
        self.root.attributes('-topmost', True)
        self.root.overrideredirect(False)

        # Window settings
        self.root.geometry("320x210+50+50")
        self.root.resizable(True, True)
        self.root.configure(bg='#1a1a2e')

        # Icon
        icon_path = Path(__file__).parent / "icons8-claude-48.png"
        self.icon_image = tk.PhotoImage(file=str(icon_path))
        self.icon_small = self.icon_image.subsample(2, 2)  # 48->24px
        self.root.iconphoto(False, self.icon_image)

        # State
        self.state_file = Path(__file__).parent / "state.json"
        self.org_uuid = None
        self.whatsapp_number = '+33XXXXXXXXX'
        self.load_state()

        # Usage data
        self.session_pct = 0
        self.session_reset = None
        self.session_expected_pct = None
        self.weekly_pct = 0
        self.weekly_reset = None
        self.weekly_expected_pct = None

        # Notification tracking
        self.session_notified = False
        self.last_session_reset = None

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

    def _draw_claude_logo(self, parent, bg_color):
        """Display the Claude logo from PNG"""
        label = tk.Label(parent, image=self.icon_small, bg=bg_color)
        return label

    def _get_pace_status(self, actual, expected):
        """Return (label, color) comparing actual vs expected usage"""
        if expected is None:
            return ("", '#a0a0a0')
        diff = actual - expected
        if diff > 5:
            return ("En avance", '#e94560')    # red - consuming too fast
        elif diff < -5:
            return ("En retard", '#f4a261')    # orange - consuming slow
        return ("On track", '#4ecca3')          # green

    def _get_bar_color(self, pct):
        """Get color based on usage percentage"""
        if pct >= 90:
            return '#e94560'
        elif pct >= 70:
            return '#f4a261'
        return '#4ecca3'

    def _create_progress_canvas(self, parent):
        """Create a canvas-based progress bar with marker support"""
        canvas = tk.Canvas(parent, height=12, bg='#16213e', highlightthickness=0)
        canvas.pack(side='left', fill='x', expand=True)
        canvas._bar_value = 0
        canvas._bar_expected = None
        canvas._bar_color = '#4ecca3'
        canvas.bind('<Configure>', lambda e: self._redraw_bar(canvas))
        return canvas

    def _redraw_bar(self, canvas):
        """Redraw a progress bar canvas with optional expected-usage marker"""
        canvas.delete('all')
        w = canvas.winfo_width()
        h = canvas.winfo_height()
        if w <= 1:
            return

        # Progress fill
        fill_w = max(0, int(w * canvas._bar_value / 100))
        if fill_w > 0:
            canvas.create_rectangle(0, 0, fill_w, h, fill=canvas._bar_color, outline='')

        # Expected usage marker (vertical line)
        if canvas._bar_expected is not None and 0 < canvas._bar_expected < 100:
            marker_x = int(w * canvas._bar_expected / 100)
            canvas.create_line(marker_x, 0, marker_x, h, fill='#ffffff', width=2)

    def _update_bar(self, canvas, value, expected, color):
        """Update bar values and redraw"""
        canvas._bar_value = value
        canvas._bar_expected = expected
        canvas._bar_color = color
        self._redraw_bar(canvas)

    def setup_ui(self):
        # Main frame
        self.main_frame = tk.Frame(self.root, bg='#1a1a2e')
        self.main_frame.pack(fill='both', expand=True, padx=8, pady=6)

        # Header - Claude logo + refresh button
        title_frame = tk.Frame(self.main_frame, bg='#16213e')
        title_frame.pack(fill='x', pady=(0, 4))

        logo_canvas = self._draw_claude_logo(title_frame, '#16213e')
        logo_canvas.pack(side='left', padx=8, pady=3)

        # Refresh button
        self.refresh_btn = tk.Button(
            title_frame,
            text="\u21bb",
            command=self.refresh_with_feedback,
            bg='#16213e',
            fg='#ffffff',
            font=('Segoe UI', 11),
            relief='flat',
            width=2,
            cursor='hand2'
        )
        self.refresh_btn.pack(side='right', padx=2, pady=2)

        # Content frame
        self.content_frame = tk.Frame(self.main_frame, bg='#1a1a2e')
        self.content_frame.pack(fill='both', expand=True)

        # --- Session (5h) section ---
        session_header = tk.Frame(self.content_frame, bg='#1a1a2e')
        session_header.pack(fill='x')

        tk.Label(
            session_header,
            text="Session (5h)",
            bg='#1a1a2e',
            fg='#a0a0a0',
            font=('Segoe UI', 9)
        ).pack(side='left')

        self.session_pace_label = tk.Label(
            session_header,
            text="",
            bg='#1a1a2e',
            fg='#a0a0a0',
            font=('Segoe UI', 8, 'italic')
        )
        self.session_pace_label.pack(side='left', padx=(6, 0))

        session_bar_frame = tk.Frame(self.content_frame, bg='#1a1a2e')
        session_bar_frame.pack(fill='x', pady=(2, 0))

        self.session_canvas = self._create_progress_canvas(session_bar_frame)

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
        self.session_reset_label.pack(anchor='e', pady=(0, 4))

        # --- Weekly section ---
        weekly_header = tk.Frame(self.content_frame, bg='#1a1a2e')
        weekly_header.pack(fill='x')

        tk.Label(
            weekly_header,
            text="Hebdomadaire",
            bg='#1a1a2e',
            fg='#a0a0a0',
            font=('Segoe UI', 9)
        ).pack(side='left')

        self.weekly_pace_label = tk.Label(
            weekly_header,
            text="",
            bg='#1a1a2e',
            fg='#a0a0a0',
            font=('Segoe UI', 8, 'italic')
        )
        self.weekly_pace_label.pack(side='left', padx=(6, 0))

        weekly_bar_frame = tk.Frame(self.content_frame, bg='#1a1a2e')
        weekly_bar_frame.pack(fill='x', pady=(2, 0))

        self.weekly_canvas = self._create_progress_canvas(weekly_bar_frame)

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
        self.weekly_reset_label.pack(anchor='e')

        # Status label
        self.status_label = tk.Label(
            self.content_frame,
            text="",
            bg='#1a1a2e',
            fg='#666666',
            font=('Segoe UI', 8)
        )
        self.status_label.pack(pady=(2, 0))

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

    def refresh_with_feedback(self):
        """Refresh with simple visual feedback"""
        self.refresh_btn.config(state='disabled', fg='#4ecca3')
        self.refresh_data()
        self.root.after(800, lambda: self.refresh_btn.config(state='normal', fg='#ffffff'))

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

        # Calculate expected usage percentages
        now = datetime.now(timezone.utc)

        if self.session_reset:
            session_start = self.session_reset - timedelta(hours=5)
            elapsed = (now - session_start).total_seconds()
            total = 5 * 3600
            self.session_expected_pct = max(0, min(100, elapsed / total * 100))

        if self.weekly_reset:
            weekly_start = self.weekly_reset - timedelta(days=7)
            elapsed = (now - weekly_start).total_seconds()
            total = 7 * 24 * 3600
            self.weekly_expected_pct = max(0, min(100, elapsed / total * 100))

        # Check if session reset changed (new session available)
        if session_reset_str != self.last_session_reset:
            if self.last_session_reset is not None and self.session_pct < 10:
                # Session was reset - send notification if we were waiting
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
            session_color = self._get_bar_color(self.session_pct)
            self.session_pct_label.config(text=f"{self.session_pct:.0f}%", fg=session_color)
            self._update_bar(self.session_canvas, self.session_pct, self.session_expected_pct, session_color)

            s_text, s_color = self._get_pace_status(self.session_pct, self.session_expected_pct)
            self.session_pace_label.config(text=s_text, fg=s_color)

            if self.session_reset:
                local_reset = self.session_reset.astimezone()
                self.session_reset_label.config(text=f"Reset: {local_reset.strftime('%H:%M')}")

            # Weekly
            weekly_color = self._get_bar_color(self.weekly_pct)
            self.weekly_pct_label.config(text=f"{self.weekly_pct:.0f}%", fg=weekly_color)
            self._update_bar(self.weekly_canvas, self.weekly_pct, self.weekly_expected_pct, weekly_color)

            w_text, w_color = self._get_pace_status(self.weekly_pct, self.weekly_expected_pct)
            self.weekly_pace_label.config(text=w_text, fg=w_color)

            if self.weekly_reset:
                local_reset = self.weekly_reset.astimezone()
                self.weekly_reset_label.config(text=f"Reset: {local_reset.strftime('%d/%m %H:%M')}")

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

            # Calculate delay
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

    def _update_expected_markers(self):
        """Recalculate expected usage markers and redraw bars"""
        now = datetime.now(timezone.utc)

        if self.session_reset:
            session_start = self.session_reset - timedelta(hours=5)
            elapsed = (now - session_start).total_seconds()
            self.session_expected_pct = max(0, min(100, elapsed / (5 * 3600) * 100))

        if self.weekly_reset:
            weekly_start = self.weekly_reset - timedelta(days=7)
            elapsed = (now - weekly_start).total_seconds()
            self.weekly_expected_pct = max(0, min(100, elapsed / (7 * 24 * 3600) * 100))

        session_color = self._get_bar_color(self.session_pct)
        self._update_bar(self.session_canvas, self.session_pct, self.session_expected_pct, session_color)
        weekly_color = self._get_bar_color(self.weekly_pct)
        self._update_bar(self.weekly_canvas, self.weekly_pct, self.weekly_expected_pct, weekly_color)

    def run(self):
        # Auto-refresh every 2 minutes
        def auto_refresh():
            self.refresh_data()
            self.root.after(120000, auto_refresh)  # 2 minutes

        # Update expected-usage markers every 5 minutes
        def update_markers():
            self._update_expected_markers()
            self.root.after(300000, update_markers)  # 5 minutes

        self.root.after(120000, auto_refresh)
        self.root.after(300000, update_markers)
        self.root.mainloop()


if __name__ == "__main__":
    app = TokenMonitor()
    app.run()
