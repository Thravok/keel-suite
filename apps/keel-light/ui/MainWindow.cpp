#include "MainWindow.h"
#include "RpcClient.h"

#include <QCheckBox>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QListWidget>
#include <QPushButton>
#include <QSlider>
#include <QVBoxLayout>
#include <QWidget>

namespace {
int miredToKelvin(int m) { return m > 0 ? 1000000 / m : 0; }
} // namespace

MainWindow::MainWindow(quint16 port, QWidget *parent)
    : QMainWindow(parent), port_(port) {
  setWindowTitle(QStringLiteral("Keel Light"));
  resize(520, 420);
  rpc_ = new RpcClient(this);
  connect(rpc_, &RpcClient::connectedChanged, this, &MainWindow::onConnected);
  buildUi();
  rpc_->connectTo(port_);
  connect(&refreshTimer_, &QTimer::timeout, this, &MainWindow::refreshLights);
  refreshTimer_.start(2000);
}

void MainWindow::buildUi() {
  auto *central = new QWidget(this);
  setCentralWidget(central);
  auto *root = new QVBoxLayout(central);

  status_ = new QLabel(QStringLiteral("Connecting…"), central);
  root->addWidget(status_);

  auto *row = new QHBoxLayout();
  list_ = new QListWidget(central);
  list_->setMinimumWidth(220);
  row->addWidget(list_, 1);

  auto *controls = new QVBoxLayout();
  power_ = new QCheckBox(QStringLiteral("Power on"), central);
  controls->addWidget(power_);

  brightnessLabel_ = new QLabel(QStringLiteral("Brightness: —"), central);
  controls->addWidget(brightnessLabel_);
  brightness_ = new QSlider(Qt::Horizontal, central);
  brightness_->setRange(0, 100);
  controls->addWidget(brightness_);

  temperatureLabel_ = new QLabel(QStringLiteral("Temperature: —"), central);
  controls->addWidget(temperatureLabel_);
  temperature_ = new QSlider(Qt::Horizontal, central);
  // ECC Wi‑Fi/USB API: 143 (cool) … 344 (warm)
  temperature_->setRange(143, 344);
  temperature_->setInvertedAppearance(true);
  controls->addWidget(temperature_);

  refreshBtn_ = new QPushButton(QStringLiteral("Refresh"), central);
  controls->addWidget(refreshBtn_);
  controls->addStretch(1);
  row->addLayout(controls, 1);
  root->addLayout(row);

  connect(list_, &QListWidget::currentRowChanged, this, &MainWindow::onSelectionChanged);
  connect(power_, &QCheckBox::toggled, this, &MainWindow::onPowerToggled);
  connect(brightness_, &QSlider::valueChanged, this, &MainWindow::onBrightnessChanged);
  connect(temperature_, &QSlider::valueChanged, this, &MainWindow::onTemperatureChanged);
  connect(refreshBtn_, &QPushButton::clicked, this, &MainWindow::refreshLights);
}

void MainWindow::onConnected(bool on) {
  status_->setText(on ? QStringLiteral("Connected · ws://127.0.0.1:%1").arg(port_)
                      : QStringLiteral("Disconnected"));
  if (on)
    refreshLights();
}

void MainWindow::refreshLights() {
  rpc_->call(
      QStringLiteral("refreshLights"), QJsonObject{},
      [this](QJsonValue result) {
        const auto lights = result.toObject().value(QStringLiteral("lights")).toArray();
        const QString prev = selectedId();
        list_->clear();
        for (const auto &v : lights) {
          const auto o = v.toObject();
          const auto info = o.value(QStringLiteral("info")).toObject();
          const QString id = info.value(QStringLiteral("id")).toString();
          const QString name = info.value(QStringLiteral("name")).toString();
          const bool on = o.value(QStringLiteral("on")).toBool();
          const int bri = o.value(QStringLiteral("brightness")).toInt();
          const int temp = o.value(QStringLiteral("temperature")).toInt();
          auto *item = new QListWidgetItem(
              QStringLiteral("%1 — %2% · %3 K")
                  .arg(name)
                  .arg(bri)
                  .arg(miredToKelvin(temp)));
          item->setData(Qt::UserRole, id);
          item->setData(Qt::UserRole + 1, o);
          item->setData(Qt::UserRole + 2, on);
          list_->addItem(item);
          if (id == prev)
            list_->setCurrentItem(item);
        }
        if (!list_->currentItem() && list_->count() > 0)
          list_->setCurrentRow(0);
        if (list_->count() == 0)
          status_->setText(QStringLiteral("No USB Key Lights (Neo / Mini HID)"));
        onSelectionChanged();
      },
      [this](QString err) { status_->setText(QStringLiteral("RPC error: %1").arg(err)); });
}

QString MainWindow::selectedId() const {
  auto *item = list_->currentItem();
  return item ? item->data(Qt::UserRole).toString() : QString();
}

void MainWindow::onSelectionChanged() {
  auto *item = list_->currentItem();
  applying_ = true;
  if (!item) {
    power_->setChecked(false);
    brightness_->setValue(0);
    temperature_->setValue(200);
    brightnessLabel_->setText(QStringLiteral("Brightness: —"));
    temperatureLabel_->setText(QStringLiteral("Temperature: —"));
    applying_ = false;
    return;
  }
  const auto o = item->data(Qt::UserRole + 1).toJsonObject();
  power_->setChecked(o.value(QStringLiteral("on")).toBool());
  const int bri = o.value(QStringLiteral("brightness")).toInt();
  const int temp = o.value(QStringLiteral("temperature")).toInt();
  brightness_->setValue(bri);
  temperature_->setValue(temp);
  brightnessLabel_->setText(QStringLiteral("Brightness: %1%").arg(bri));
  temperatureLabel_->setText(
      QStringLiteral("Temperature: %1 K (mired %2)").arg(miredToKelvin(temp)).arg(temp));
  applying_ = false;
}

void MainWindow::onPowerToggled(bool on) {
  if (applying_)
    return;
  const QString id = selectedId();
  if (id.isEmpty())
    return;
  rpc_->call(QStringLiteral("setLightPower"),
             QJsonObject{{"id", id}, {"on", on}},
             [this](QJsonValue) { refreshLights(); });
}

void MainWindow::onBrightnessChanged(int value) {
  brightnessLabel_->setText(QStringLiteral("Brightness: %1%").arg(value));
  if (applying_)
    return;
  applyControls();
}

void MainWindow::onTemperatureChanged(int mired) {
  temperatureLabel_->setText(
      QStringLiteral("Temperature: %1 K (mired %2)").arg(miredToKelvin(mired)).arg(mired));
  if (applying_)
    return;
  applyControls();
}

void MainWindow::applyControls() {
  const QString id = selectedId();
  if (id.isEmpty())
    return;
  rpc_->call(QStringLiteral("setLightBrightness"),
             QJsonObject{{"id", id}, {"brightness", brightness_->value()}},
             [](QJsonValue) {});
  rpc_->call(QStringLiteral("setLightTemperature"),
             QJsonObject{{"id", id}, {"temperature", temperature_->value()}},
             [](QJsonValue) {});
}
