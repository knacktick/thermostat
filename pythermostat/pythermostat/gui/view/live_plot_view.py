from collections import deque
from PyQt6.QtCore import QObject, pyqtSlot
from pglive.sources.data_connector import DataConnector
from pglive.kwargs import Axis
from pglive.sources.live_plot import LiveLinePlot
from pglive.sources.live_axis import LiveAxis
import pyqtgraph as pg
from pythermostat.gui.model.thermostat import ThermostatConnectionState


pg.setConfigOptions(antialias=True)


class LiveDataPlotter(QObject):
    def __init__(self, thermostat, live_plots):
        super().__init__()
        self._thermostat = thermostat

        self._thermostat.report_update.connect(self.update_report)
        self._thermostat.pid_update.connect(self.update_pid)
        self._thermostat.connection_state_update.connect(
            self.thermostat_state_change_handler
        )

        self.NUM_CHANNELS = len(live_plots)
        self.graphs = []

        for i, live_plot in enumerate(live_plots):
            live_plot[0].setTitle(f"Channel {i} Temperature")
            live_plot[1].setTitle(f"Channel {i} Current")
            self.graphs.append(_TecGraphs(live_plot[0], live_plot[1]))

    @pyqtSlot(ThermostatConnectionState)
    def thermostat_state_change_handler(self, state):
        if state == ThermostatConnectionState.DISCONNECTED:
            self.clear_graphs()

    def _config_connector_max_pts(self, connector, samples):
        connector.max_points = samples
        connector.x = deque(maxlen=int(connector.max_points))
        connector.y = deque(maxlen=int(connector.max_points))

    @pyqtSlot(int)
    def set_max_samples(self, samples: int):
        for graph in self.graphs:
            self._config_connector_max_pts(graph.t_connector, samples)
            self._config_connector_max_pts(graph.i_connector, samples)
            self._config_connector_max_pts(graph.iset_connector, samples)

    @pyqtSlot()
    def clear_graphs(self):
        for graph in self.graphs:
            graph.clear()

    @pyqtSlot(list)
    def update_pid(self, pid_settings):
        for settings in pid_settings:
            channel = settings["channel"]
            self.graphs[channel].update_pid(settings)

    @pyqtSlot(list)
    def update_report(self, report_data):
        for settings in report_data:
            channel = settings["channel"]
            self.graphs[channel].update_report(settings)


class _TecGraphs:
    """The maximum number of sample points to store."""

    DEFAULT_MAX_SAMPLES = 1000

    def __init__(self, t_widget, i_widget):
        self._t_widget = t_widget
        self._i_widget = i_widget

        self._t_plot = LiveLinePlot()
        self._i_plot = LiveLinePlot(name="Measured")
        self._iset_plot = LiveLinePlot(name="Set", pen=pg.mkPen("r"))

        self._t_line = self._t_widget.getPlotItem().addLine(label="{value} °C")
        self._t_line.setVisible(False)
        # Hack for keeping setpoint line in plot range
        self._t_setpoint_plot = LiveLinePlot()

        for graph in t_widget, i_widget:
            time_axis = LiveAxis(
                "bottom",
                text="Time since Thermostat reset",
                **{Axis.TICK_FORMAT: Axis.DURATION},
            )
            time_axis.showLabel()
            graph.setAxisItems({"bottom": time_axis})

            graph.add_crosshair(pg.mkPen(color="red", width=1), {"color": "green"})

            # Enable linking of axes in the graph widget's context menu
            graph.register(
                graph.getPlotItem().titleLabel.text  # Slight hack getting the title
            )

        temperature_axis = LiveAxis("left", text="Temperature", units="°C")
        temperature_axis.showLabel()
        t_widget.setAxisItems({"left": temperature_axis})

        current_axis = LiveAxis("left", text="Current", units="A")
        current_axis.showLabel()
        i_widget.setAxisItems({"left": current_axis})
        i_widget.addLegend(brush=(50, 50, 200, 150))

        t_widget.addItem(self._t_plot)
        t_widget.addItem(self._t_setpoint_plot)
        i_widget.addItem(self._i_plot)
        i_widget.addItem(self._iset_plot)

        self.t_connector = DataConnector(
            self._t_plot, max_points=self.DEFAULT_MAX_SAMPLES
        )
        self.t_setpoint_connector = DataConnector(self._t_setpoint_plot, max_points=1)
        self.i_connector = DataConnector(
            self._i_plot, max_points=self.DEFAULT_MAX_SAMPLES
        )
        self.iset_connector = DataConnector(
            self._iset_plot, max_points=self.DEFAULT_MAX_SAMPLES
        )

        self.max_samples = self.DEFAULT_MAX_SAMPLES

    def plot_append(self, report):
        temperature = report["temperature"]
        current = report["tec_i"]
        iset = report["i_set"]
        time = report["time"]

        if temperature is not None:
            self.t_connector.cb_append_data_point(temperature, time)
            if self._t_line.isVisible():
                self.t_setpoint_connector.cb_append_data_point(
                    self._t_line.value(), time
                )
            else:
                self.t_setpoint_connector.cb_append_data_point(temperature, time)
            if current is not None:
                self.i_connector.cb_append_data_point(current, time)
            self.iset_connector.cb_append_data_point(iset, time)

    def set_max_sample(self, samples: int):
        for connector in self.t_connector, self.i_connector, self.iset_connector:
            connector.max_points(samples)

    def clear(self):
        for connector in self.t_connector, self.i_connector, self.iset_connector:
            connector.clear()

    def set_t_line(self, temp=None, visible=None):
        if visible is not None:
            self._t_line.setVisible(visible)
        if temp is not None:
            self._t_line.setValue(temp)

            # PyQtGraph normally does not update this text when the line
            # is not visible, so make sure that the temperature label
            # gets updated always, and doesn't stay at an old value.
            self._t_line.label.setText(f"{temp} °C")

    def set_max_samples(self, samples: int):
        for graph in self.graphs:
            graph.t_connector.max_points = samples
            graph.i_connector.max_points = samples
            graph.iset_connector.max_points = samples

    def clear_graphs(self):
        for graph in self.graphs:
            graph.clear()

    def update_pid(self, pid_settings):
        self.set_t_line(temp=round(pid_settings["target"], 6))

    def update_report(self, report_data):
        self.plot_append(report_data)
        self.set_t_line(visible=report_data["pid_engaged"])
