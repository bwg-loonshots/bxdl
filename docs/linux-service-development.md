# Linux 개발용 systemd 서비스

LNX-03은 **관리자가 명시적으로 등록하는 개발용 경로**다. 기존 Mac 등록 형식과 권한 규칙은 유지한다. Linux 순차 설치·계정 준비·이름 기반 운용 UX, signed runtime archive와 고정 내부 실행기 profile은 LNX-04에서 연결한다. 이 문서는 고객 설치 안내가 아니다.

실제 Linux 실행 결과는 **NOT_RUN**이다. Mac의 회귀 검사와 Linux target 타입 검사를 실제 계정 생성·권한 격리·systemd·RocksDB 검증으로 계산하지 않는다. SSH 접속 정보를 개발자에게 제공할 필요는 없다. 관리자가 아래 준비와 별도 실물 시험을 수행하고 정제한 결과를 반환할 수 있다.

## 대상과 권한

- Ubuntu 24.04 LTS, x86_64/glibc, PID 1 systemd 255 이상. 등록·초기화·시작 gate는 system manager의 `running` 상태도 요구한다. WSL2는 후속이다.
- 고정 `/usr/bin/systemctl`, `/usr/bin/setpriv`를 사용한다. shell/PATH 탐색이나 자동 sudo 요청은 하지 않는다.
- root가 관리하는 CLI executable과 persistent instance parent가 필요하다. 사용자 home·공용 `/tmp` 아래를 system service 제어 경로로 사용하지 않는다.
- 로컬 `/etc/passwd`·`/etc/group`에 전용 nologin 계정과 비어 있는 전용 그룹을 인스턴스마다 사전 생성한다. LDAP/SSSD 등 NSS 전체의 계정 검증을 제공하지 않는다. 일반 로그인 계정·root UID/GID·다른 인스턴스 계정을 재사용하지 않는다.
- LNX-03의 등록·init·start·status·stop은 명시적인 관리자 실행을 요구한다. 사용자별 `~/.local/share/BXDL` catalog에 root 기록을 섞지 않는다. sudoers나 정책 파일을 자동 수정하지 않는다.

## 등록 준비

먼저 `bxdl host check --json`으로 읽기 전용 기준 검사를 한다. manager가 `degraded`이면 등록·init·새 시작은 거절한다. 이미 실행한 시도의 `status/stop`은 이 제한을 다시 적용하지 않아 종료 관측과 정리를 시도할 수 있다. host 성공은 엔진·native·계정 격리 인수 성공을 뜻하지 않는다.

LNX-02의 [고정 공급물](../packaging/linux-rc4-candidate.json)과 [Linux 실행 검증 안내](./linux-validation.md)를 사용한다. Java/JAR의 cold 결과만으로 native/RocksDB 검증을 완료했다고 판단하지 않는다. CLI는 native Linux에서 빌드한다. Mac에서 생성한 CLI는 사용하지 않는다.

관리자가 검토한 CLI를 root가 보호하는 위치에 설치한다. 아래는 경로 예시다. 기존 파일/계정에 덮어쓰지 않고 새 시험 대상인지 먼저 확인한다.

```sh
sudo install -d -o root -g root -m 0755 /opt/bxdl/development /var/lib/bxdl/instances
sudo install -o root -g root -m 0755 target/release/bxdl /opt/bxdl/development/bxdl
/opt/bxdl/development/bxdl linux-service account --instance /var/lib/bxdl/instances/linux-dev-1 --json
```

마지막 명령은 계정 이름을 계산할 뿐 계정·파일·서비스를 만들지 않는다. 반환된 `data.account`를 다음 명령의 `<계정>`에 사용한다. `--instance` 경로는 등록까지 동일하게 유지한다. 현재 unit 경로는 공백·한글·systemd 확장 문자가 없는 정규 절대 경로를 사용한다.

```sh
sudo groupadd --system <계정>
sudo useradd --system --gid <계정> --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin <계정>
id <계정>
```

다음 형식의 spec을 준비한다. UID/GID는 예시 숫자를 그대로 쓰지 않고 실제 전용 계정 값을 입력한다. 모든 경로는 절대 경로다. spec 파일과 그 상위 경로는 root 소유이며 group/other가 쓰지 못해야 한다. 제품/native 설정은 동일한 체인·키·포트·새 데이터 경로를 가리켜야 하며, native 설정의 `dataDirectory`는 새 instance의 `data` 경로여야 한다. 기존 운영 데이터는 가져오지 않는다.

```json
{
  "schemaVersion": 1,
  "runtimeRoot": "/srv/bxdl-input/runtime",
  "runtimeLock": "/srv/bxdl-input/runtime-lock.temurin-21-linux-amd64.json",
  "engineJar": "/srv/bxdl-input/nigo-node-0.0.1-SNAPSHOT.jar",
  "engineLock": "/srv/bxdl-input/engine-lock.rc4-temurin-linux-development.json",
  "nativeConfig": "/srv/bxdl-input/node.json",
  "productConfig": "/srv/bxdl-input/instance.json",
  "serviceUid": 991,
  "serviceGid": 991
}
```

선정 runtime/engine lock의 bytes, RC.4 JAR/Java hash와 전체 JRE inventory를 대조한다. 입력은 root 제어 영역에 보관한 뒤 사용하고 Java는 root로 실행하지 않는다. 원래 입력 경로를 계속 실행 권한의 근거로 쓰지 않는다. 등록 중 실패한 새 폴더는 자동 삭제하거나 기존 정상 등록으로 채택하지 않는다.

```sh
sudo /opt/bxdl/development/bxdl linux-service register \
  --instance /var/lib/bxdl/instances/linux-dev-1 \
  --spec /srv/bxdl-input/linux-service.json --allow-development --json
```

등록은 명시 초기화·서비스 시작과 별도다. `--allow-development`는 정식 공급·Linux 인수가 끝났다는 승인이 아니다. 이 경로에서 보관한 개발용 worker를 signed Linux package의 승인된 exact executor로 간주하지 않는다. 등록 당시 worker를 이후 시도의 gate로 복사하며 관리 CLI 파일 교체가 기존 worker를 바꾸지 않는다. init/제어 기록 작업 전체를 고정 A-L에 위임하는 외부 CLI 호환 profile은 아직 없으므로 독립 C-L/R-L 조합 인수 완료를 뜻하지 않는다.

## 초기화·운용

```sh
sudo /opt/bxdl/development/bxdl init --instance /var/lib/bxdl/instances/linux-dev-1 --confirm-initialize --json
sudo /opt/bxdl/development/bxdl start --instance /var/lib/bxdl/instances/linux-dev-1 --json
sudo /opt/bxdl/development/bxdl status --instance /var/lib/bxdl/instances/linux-dev-1 --wait --json
sudo /opt/bxdl/development/bxdl stop --instance /var/lib/bxdl/instances/linux-dev-1 --json
```

`resume-init --confirm-resume`은 같은 identity의 미완료 초기화 기록과 잠금 조건을 충족할 때만 사용한다. 초기화 불명 기록을 지우거나 DB를 비운 뒤 성공한 것으로 바꾸지 않는다. 시작 대기 시간 만료는 JVM 종료를 뜻하지 않으며 상태를 다시 관측한다. 자동 재시작과 부팅 enable은 제공하지 않는다.

시도별 systemd unit은 root 소유 worker의 `systemd-run` gate를 호출한다. 직접 `systemctl start`해도 같은 gate를 거치고 소비된 승인은 재사용할 수 없다. 운영 시작은 새 시도를 만드는 `bxdl start`를 사용한다. `systemd-run`과 `systemd-exit`은 내부 명령이므로 운영자가 직접 호출할 필요가 없다. `ExecStopPost`에서 root가 InvocationID/ControlPID/cgroup을 확인하고 OS 종료 기록을 남겨, systemd가 종료 unit의 메모리 정보를 정리한 뒤에도 결속을 확인한다. 종료 기록만으로 엔진 정상 종료를 인정하지 않는다. unit의 drop-in/ExecStart 변경을 자동 수용하지 않는다.

root gate는 unit/InvocationID/PID/cgroup과 root 기록을 결속한다. 고정 `setpriv`가 보조 그룹·capability를 제거하고 전용 UID/GID로 Java를 exec하여 PID를 유지한다. 설정·공급물·journal은 서비스 UID가 수정할 수 없으며 data와 지정 output만 서비스 UID가 소유한다. init/cold에도 같은 권한 전환을 적용한다. root 제어 기록의 쓰기 descriptor는 엔진에 전달하지 않는다.

`active/running`은 로컬 READY가 아니다. 상태는 엔진 report·nodeInstanceId·HTTP bootstrap/health와 process identity를 결합한다. 정상 종료는 해당 엔진의 STOPPED/storage·consensus closure, process/cgroup 부재와 DB lock 해제 조건이 모두 필요하다. 종료 요청 수락만으로 성공을 기록하지 않으며 근거가 모자라면 UNKNOWN으로 보존한다. 이 모든 관측은 네트워크 정족수·거래 확정과 별개다.

## 제한된 저장 기록·진단

```sh
sudo /opt/bxdl/development/bxdl logs --instance /var/lib/bxdl/instances/linux-dev-1 --json
sudo /opt/bxdl/development/bxdl diagnose --instance /var/lib/bxdl/instances/linux-dev-1 --output /var/lib/bxdl/linux-dev-1-diagnostic.json --json
```

LNX-03은 root 제어 상태와 검증 가능한 최종 엔진 report의 요약만 반환한다. HTTP·systemctl·Java를 실행하지 않으며 runtime 상태는 `NOT_CHECKED`다. 전체 사건 이력·system journal·raw stdout/stderr를 수집하지 않으므로 항상 `partial=true`, `INCOMPLETE`(exit 5)로 표시한다. `--tail`은 현재 전체 사건 수집을 켜지 않는다. 비밀·제어 경로·원문을 출력하지 않으며 누락 report를 수집 완료로 표시하지 않는다.

`diagnose`는 제어/data 영역 밖의 새 파일에만 root0600으로 저장한다. root가 보호하는 상위 디렉터리를 사용하며 기존 파일을 덮거나 symlink를 따라가지 않는다. 이 보고서는 저장 자료 수집 결과이고 노드 안전·정상 운용 판정이 아니다. 전체 bounded event collector와 일반 사용자 접근 UX는 LNX-04에 연결한다.

## 실물 인수에서 확인할 항목

1. 실제 OS/kernel/glibc/systemd, CLI source/hash, runtime/JAR/Java hash를 기록하고 LNX-02 cold/native 시험을 수행한다.
2. root JVM이 없는지, 엔진 UID/GID·보조 그룹·capability를 관측한다. 엔진 계정에서 root journal/worker/config와 다른 인스턴스 data를 변경하지 못하는지 확인한다.
3. 신규 등록·init·start·ready·stop·같은 DB 재시작을 수행한다. unit/InvocationID/PID/cgroup/nodeInstanceId와 report를 같은 시도로 대조한다.
4. 미초기화·중단된 init·변조된 입력·소비한 attempt의 직접 `systemctl start`가 거절되는지 확인한다. 다른 unit/PID에 종료 신호를 보내지 않는지 확인한다.
5. 초기화 중 CLI 중단·서비스 startup 실패·종료 timeout에서 UNKNOWN/잠금/원자료를 보존하고 중복 JVM을 만들지 않는지 확인한다.
6. SSH 종료 뒤 같은 시도의 지속 동작, 정상 종료 이후 cgroup/PID/포트 부재, DB 잠금 해제를 확인한다. 재부팅·강제 종료는 별도 시험이며 정상 stop 결과로 대신하지 않는다.
7. Linux 설치/계정 준비/이름 연결을 통합한 뒤 작성자가 아닌 테스터에게 짧은 안내서로 사용자 과업을 배정한다.

검증되지 않은 사례를 PASS로 채우지 않는다. 공개 결과에는 비밀·사용자 경로·인증정보를 넣지 않으며 원자료는 시험 장비의 제한된 폴더에 보관한다. 기존 Mac 결과와 LNX-02 고정 전달 archive는 변경하지 않는다.

## 참고

- [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html): `Type=exec`, `Restart`와 서비스 시작 의미.
- [systemd.kill](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html): `KillMode=control-group`, `SendSIGKILL=no`와 stop timeout.
- [util-linux setpriv 명세](https://kernel.googlesource.com/pub/scm/utils/util-linux/util-linux/+/refs/tags/v2.41.1/sys-utils/setpriv.1.adoc): UID/GID·보조 그룹·capability·no_new_privs 전환과 실패 시 실행 거부.
