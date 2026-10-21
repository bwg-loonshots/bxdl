# NIGO engine RC.2 수신 — 2026-10-21

BXDL이 private GitHub prerelease에서 직접 받은 `nigo-engine-v0.1.0-rc.2`의 정적 수신 검증과 명시 선택 기록이다.
사용자의 RC.2 반영 요청에 따라 Mac arm64·선정 Temurin21·RocksDB 개발 패키지 재인수 후보로 선택했다.
기존 RC.1 lock·snapshot·결과는 이력으로 보존한다. 원본 계약·manifest·공급자 evidence는 bytes 그대로 보존한다.
이 README, `consumer-receipt.json`, snapshot용 `SHA256SUMS`만 BXDL 기록이다.

## 선택한 후보

| 항목 | 값 |
| --- | --- |
| 인계 문서 revision | `7f2efdfb72a0e024ba7ff80dba7b09a4533e64bc` |
| 공급 라인 / 태그 | `release/engine-0.1` / `nigo-engine-v0.1.0-rc.2` |
| 태그 object / source | `b2abbc8f9923c5cc465fb0eb66ef2f88242873ab` / `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b` |
| source dirty | `false` |
| JAR SHA-256 / bytes | `b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10` / 164,085,159 |
| Archive SHA-256 / bytes | `abab5669e9c6aa991332a908a8f991b7191179a99fc3799619ae2c8304e5ff22` / 158,091,837 |
| Manifest SHA-256 | `254a1bd33499f40d455fef451f80abb83e81eea66095ee68d796746dd7e0d10c` |
| 계약 fingerprint | `bac1756a81f2d2ddf67ae094fc770bc67060482acab87da7d48b93d441ab8f88` |
| Console fingerprint | `4df95293576a4baaa63639d3540ae6657b89c0eae80b1e2e2f7f9e6412a32c73` |
| 선정 runtime | Eclipse Adoptium Temurin `21.0.12.1+1-LTS` full JRE, macOS arm64 |
| Java launcher SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| 공급 채널 / 계약 | `development`, `officialRelease=false`, `PROPOSED` |
| BXDL 기반 / 작업 브랜치 | `2955ba5cdedeaf6dba65a98a905588e71770abea` / `codex/rc2-macos-acceptance` |

[공급 release](https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.2)의
release ID는 `419161098`, archive/checksum asset ID는 `661349278`/`661349277`이다.
서버 release는 `immutable=false`이며, checksum만으로 배포자 서명을 검증했다고 주장하지 않는다.
이 정적 검증은 앞선 BXDL 원격 수신·리뷰에서 확보한 archive를 다시 대조했다. 원격 API·서명 검증의 신규 실행은 아니다.

## 수신 검증과 원문 보존

- trusted archive hash·크기와 sidecar checksum이 일치했고 gzip payload 전체를 읽었다.
- 상대 경로 정규형·고정 최상위 디렉터리·중복·link/special file을 검사한 뒤 별도 경로에 안전하게 추출했다.
  일반 파일 19개 중 `SHA256SUMS`를 제외한 18개 파일이 inventory와 빠짐없이 일치했다.
- JAR hash·크기, 원본 manifest, JAR 내 build JSON·console JSON과 공급 `engine-info` 전체 identity를 대조했다.
  이 단계는 JVM을 실행한 `engine-info` 검증이 아니다.
- 계약 4개의 archive·JAR 내 원문·고정 source commit을 비교했고 `sourcePath + NUL + bytes + NUL` 순서로
  fingerprint를 재계산했다. 두 native 운영 문서도 같은 source와 일치했다.
- 공급 test-summary의 개별 case를 다시 집계해 canonical source **1,262 PASS**, 별도 집중 source **348 PASS**,
  exact JAR **9 PASS**, 기존 opt-in **2 NOT_RUN**을 확인했다. 중복 실행을 합산하거나 BXDL 제품 시험으로 표시하지 않는다.
- 정적 판정은 [consumer-receipt.json](./consumer-receipt.json)에 고정한다. `consumerPackageAcceptance=NOT_RUN`은
  이 수신 단계의 범위다. 실제 선정 JRE 실행과 Mac 패키지 인수 결과는 별도 `results/` 기록으로 남기며 원문을 덮어쓰지 않는다.

공급자의 원래 README와 checksum은 각각 `supplier-README.md`, `supplier-SHA256SUMS`로 보존했다.
원문 상대 링크와 경로는 NIGO 저장소/공급 bundle 기준이며 수정하지 않았다. supplier checksum은 JAR를 포함한
원래 18개 inventory이므로 평탄화한 이 디렉터리에 바로 적용할 수 없다. 이 디렉터리의 `SHA256SUMS`는
BXDL snapshot 19개 파일의 별도 inventory이며 자신을 제외한다.

JAR·archive·JRE·PKI·DB·키·비밀번호·미정제 로그는 Git에 넣지 않는다. 원본 manifest의 `NOT_RECORDED`,
`NOT_RUN`, `NOT_CHECKED`, `PROPOSED`를 공급자 후속 시험 결과로 덮어쓰지 않는다. supplier evidence의 게시 전
상태도 그대로 보존하고 실제 배포 identity와 BXDL 결과는 소비자 기록에서 구분한다.

## RC.1과의 차이와 선택 방법

QBFT 상위 round-change 증거 추종 보완과 비용 시험 실행 조건 변경을 포함한 새 source/JAR다.
계약 4개와 fingerprint, Console fingerprint, 선정 Java launcher는 RC.1과 같다. 그렇더라도 source/JAR가
달라졌으므로 RC.1의 인수 PASS는 자동 승계하지 않는다. 과거 비용 gate 실패의 직접 원인은 `UNRESOLVED`이며,
새 fresh-JVM 시험 조건의 통과를 원인 규명이나 운영 성능 보장으로 해석하지 않는다.

- [RC.2 Temurin engine lock](../../../packaging/engine-lock.rc2-temurin-development.json)
- [RC.2 Mac package spec](../../../packaging/package-spec.rc2-temurin.example.json)

lock의 `expected`는 수신한 `evidence/engine-info.json` 전체 identity와 같다. package를 조립할 때 실제 BXDL CLI,
JRE 전체 inventory·프로필 입력과 package hash를 별도로 결속한다. 이 파일 선택만으로 기존 설치·서비스·DB를
교체하지 않는다. 새 시험용 데이터에서 설치·명시 init·기동·정상 종료·동일 데이터 재시작과4-validator 정족수
상실/복구·후속 거래를 다시 검증한다. 공급자의 RC.1→RC.2 동일 DB 복구 시험은 BXDL upgrade UX 인수가 아니다.

Mac 전체 G1-M, JRE 출처·SBOM/NOTICE/source·고객 재배포 승인, 기존 설치 upgrade/downgrade,
Linux/systemd·Docker·multi-host·장기 운영 및 Native kit 전체 기능 인수는 별도다. REQ-0003 전체 VERIFIED나
정식 운영 출시를 주장하지 않는다.

## 후속 BXDL 실행 기록

정적 수신 뒤 같은 공급 bytes와 선정 Temurin으로 [Mac RC.2 package 재인수](../../../results/2026-10-21-macos-rc2-package.md)를 수행했다. 지정4-validator·두 복구 경로·wrong-pin 및 정상 종료는 PASS다. 이 후속 기록으로 원본 manifest나 정적 receipt의 당시 NOT_RUN을 덮어쓰지 않는다. 기존 설치 upgrade·전체 G1-M/J5·정식 출시는 별도다.
